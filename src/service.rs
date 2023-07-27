use crate::{Error, Result};
use entity::{invoice, user};
use lightning_client::{lightning, Lightning};
use rand::RngCore;
use sea_orm::{
    sea_query::Expr, ActiveModelTrait, ColumnTrait, DbConn, EntityTrait, NotSet, QueryFilter, Set,
    TransactionTrait,
};
use sha2::{Digest, Sha256};

pub fn rand_preimage() -> Vec<u8> {
    let mut store_key_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut store_key_bytes);
    store_key_bytes.to_vec()
}

/// Lightning service
pub struct Service {
    lightning: Box<dyn Lightning + Sync + Send>,
    conn: DbConn,
    name: String,
}

impl Service {
    pub fn new(name: String, lightning: Box<dyn Lightning + Sync + Send>, conn: DbConn) -> Self {
        Self {
            name,
            lightning,
            conn,
        }
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    pub fn conn(&self) -> &DbConn {
        &self.conn
    }

    pub async fn info(&self) -> Result<lightning::Info> {
        Ok(self.lightning.get_info().await?)
    }

    pub async fn get_user(&self, pubkey: Vec<u8>) -> Result<Option<user::Model>> {
        Ok(user::Entity::find()
            .filter(user::Column::Pubkey.eq(pubkey))
            .one(&self.conn)
            .await?)
    }

    pub async fn get_or_create_user(&self, pubkey: Vec<u8>) -> Result<user::Model> {
        match self.get_user(pubkey.clone()).await? {
            Some(u) => Ok(u),
            None => {
                // create
                Ok(user::ActiveModel {
                    pubkey: Set(pubkey.clone()),
                    ..Default::default()
                }
                .insert(self.conn())
                .await?)
            }
        }
    }

    pub async fn create_invoice(
        &self,
        user: &user::Model,
        memo: String,
        msats: u64,
        expiry: u64,
    ) -> Result<invoice::Model> {
        let preimage = rand_preimage();
        let mut hasher = Sha256::new();
        hasher.update(preimage.clone());
        let hash = hasher.finalize().to_vec();
        let invoice = self
            .lightning
            .create_invoice(memo.clone(), msats, Some(preimage.clone()), Some(expiry))
            .await?;

        if &invoice.payment_hash != &hash {
            return Err(Error::Str("invalid payment hash"));
        }
        let i = active_model_from_invoice(user, preimage, invoice)
            .insert(self.conn())
            .await?;
        Ok(i)
    }

    pub async fn pay(&self, user: &user::Model, bolt11: String) -> Result<invoice::Model> {
        let inv = lightning::Invoice::from_bolt11(bolt11.clone())?;

        let payment_hash = inv.payment_hash.clone();
        // todo: cal fee
        let lock_amount = inv.amount;

        let mut invoice = active_model_from_invoice(&user, vec![], inv);
        // payment
        invoice.r#type = Set(1);
        invoice.lock_amount = Set(lock_amount);

        let txn = self.conn.begin().await?;
        // lock balance
        let res = user::Entity::update_many()
            .col_expr(
                user::Column::Balance,
                Expr::col(user::Column::Balance).sub(lock_amount),
            )
            .col_expr(
                user::Column::LockAmount,
                Expr::col(user::Column::LockAmount).add(lock_amount),
            )
            .filter(user::Column::Pubkey.eq(user.pubkey.clone()))
            .filter(user::Column::Balance.gte(lock_amount))
            .exec(&txn)
            .await?;
        if res.rows_affected != 1 {
            return Err(Error::Str("The balance is insufficient or locked."));
        }

        // create payment
        let model = invoice.insert(&txn).await?;
        txn.commit().await?;

        // try pay
        let pay = self.lightning.pay(bolt11).await;
        let payment = self.lightning.lookup_payment(payment_hash).await;

        match payment {
            Ok(p) => {
                match p.status {
                    lightning::PaymentStatus::Succeeded => {
                        pay_success(&self.conn(), p, model).await
                    }
                    lightning::PaymentStatus::Failed => {
                        // failed
                        pay_failed(self.conn(), model).await?;
                        Err(pay
                            .err()
                            .map(Error::from)
                            .unwrap_or(Error::Str("pay failed")))
                    }
                    _ => {
                        Err(Error::Str("Payment in progress"))
                        // will handle by the task.
                    }
                }
            }

            Err(lightning_client::Error::PaymentNotFound) => {
                pay_failed(self.conn(), model).await?;
                Err(pay
                    .err()
                    .map(Error::from)
                    .unwrap_or(Error::Str("pay failed")))
                // failed
            }
            // will handle by the task.
            Err(e) => Err(e.into()),
        }
    }
}

async fn pay_failed(conn: &DbConn, invoice: invoice::Model) -> Result<()> {
    let lock_amount = invoice.lock_amount;

    let update = invoice::ActiveModel {
        status: Set(2),
        lock_amount: Set(0),
        ..Default::default()
    };

    let txn = conn.begin().await?;

    let res = invoice::Entity::update_many()
        .set(update)
        .filter(invoice::Column::Id.eq(invoice.id))
        .filter(invoice::Column::LockAmount.eq(lock_amount))
        .exec(&txn)
        .await?;
    // check had updated
    if res.rows_affected == 1 {
        // update user lock balance
        let _res = user::Entity::update_many()
            .col_expr(
                user::Column::Balance,
                Expr::col(user::Column::Balance).add(lock_amount),
            )
            .col_expr(
                user::Column::LockAmount,
                Expr::col(user::Column::LockAmount).sub(lock_amount),
            )
            .filter(user::Column::Pubkey.eq(invoice.user_pubkey.clone()))
            .filter(user::Column::LockAmount.gte(lock_amount))
            .exec(&txn)
            .await?;
        // if res.rows_affected != 1 {
        // }
    }
    txn.commit().await?;

    Ok(())
}
async fn pay_success(
    conn: &DbConn,
    payment: lightning::Payment,
    invoice: invoice::Model,
) -> Result<invoice::Model> {
    let lock_amount = invoice.lock_amount;

    let update = invoice::ActiveModel {
        status: Set(1),
        lock_amount: Set(0),
        amount: Set(payment.amount),
        paid_amount: Set(payment.amount),
        fee: Set(payment.fee),
        total: Set(payment.total),
        paid_at: Set(payment.created_at),
        ..Default::default()
    };

    let txn = conn.begin().await?;

    let res = invoice::Entity::update_many()
        .set(update)
        .filter(invoice::Column::Id.eq(invoice.id))
        .filter(invoice::Column::LockAmount.eq(lock_amount))
        .exec(&txn)
        .await?;
    // check had updated
    if res.rows_affected == 1 {
        // update user lock balance
        let _res = user::Entity::update_many()
            .col_expr(
                user::Column::LockAmount,
                Expr::col(user::Column::LockAmount).sub(lock_amount),
            )
            .filter(user::Column::Pubkey.eq(invoice.user_pubkey.clone()))
            .filter(user::Column::LockAmount.gte(lock_amount))
            .exec(&txn)
            .await?;
        // if res.rows_affected != 1 {
        // }
    }
    txn.commit().await?;

    invoice::Entity::find_by_id(invoice.id)
        .one(conn)
        .await?
        .ok_or(Error::Str("where is the invoice?"))
}

fn active_model_from_invoice(
    user: &user::Model,
    preimage: Vec<u8>,
    invoice: lightning::Invoice,
) -> invoice::ActiveModel {
    invoice::ActiveModel {
        id: NotSet,
        user_id: Set(user.id),
        user_pubkey: Set(user.pubkey.clone()),
        payee: Set(invoice.payee),
        r#type: Set(0),
        status: Set(0),
        payment_hash: Set(invoice.payment_hash),
        payment_preimage: Set(preimage),
        created_at: Set(invoice.created_at),
        expiry: Set(invoice.expiry),
        description: Set(invoice.description),
        bolt11: Set(invoice.bolt11),
        amount: Set(invoice.amount),
        paid_at: Set(0),
        paid_amount: Set(invoice.amount),
        fee: Set(0),
        total: Set(invoice.amount),
        lock_amount: Set(0),
    }
}
