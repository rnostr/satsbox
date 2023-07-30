use crate::{setting::Fee, Error, Result};
use entity::{invoice, user};
use lightning_client::{lightning, Lightning};
use rand::RngCore;
use sea_orm::{
    sea_query::Expr, ActiveModelTrait, ColumnTrait, DbConn, EntityTrait, NotSet, QueryFilter, Set,
    TransactionTrait,
};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

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
            .one(self.conn())
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
        let i = create_invoice_active_model(user, preimage, invoice)
            .insert(self.conn())
            .await?;
        Ok(i)
    }

    pub async fn pay(
        &self,
        user: &user::Model,
        bolt11: String,
        fee: &Fee,
        ignore_result: bool,
    ) -> Result<invoice::Model> {
        let inv = lightning::Invoice::from_bolt11(bolt11.clone())?;
        let info = self.lightning.get_info().await?;
        if info.id.eq(&inv.payee) {
            // internal payment
            internal_pay(&self.conn, user, inv, fee).await
        } else {
            // external payment
            let payment_hash = inv.payment_hash.clone();

            let amount = inv.amount;
            let (max_fee, service_fee) = fee.cal(amount, false);
            let total = amount + max_fee + service_fee;
            if user.balance < total {
                return Err(Error::Str("The balance is insufficient."));
            }

            let mut invoice = create_invoice_active_model(&user, vec![], inv);
            // payment
            invoice.r#type = Set(invoice::Type::Payment);
            invoice.lock_amount = Set(total);
            invoice.service_fee = Set(service_fee);

            let txn = self.conn.begin().await?;
            // lock balance
            let res = user::Entity::update_many()
                .col_expr(
                    user::Column::Balance,
                    Expr::col(user::Column::Balance).sub(total),
                )
                .col_expr(
                    user::Column::LockAmount,
                    Expr::col(user::Column::LockAmount).add(total),
                )
                .filter(user::Column::Id.eq(user.id))
                .filter(user::Column::Balance.gte(total))
                .exec(&txn)
                .await?;
            if res.rows_affected != 1 {
                return Err(Error::Str("The balance is insufficient or locked."));
            }

            // create payment
            let model = invoice.insert(&txn).await?;
            txn.commit().await?;

            // try pay
            let pay = self.lightning.pay(bolt11, Some(max_fee)).await;

            // don't check payment result
            if ignore_result {
                return Ok(model);
            }

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
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

async fn internal_pay(
    conn: &DbConn,
    user: &user::Model,
    inv: lightning::Invoice,
    fee: &Fee,
) -> Result<invoice::Model> {
    let payment_hash = inv.payment_hash.clone();
    let amount = inv.amount;
    let (fee, service_fee) = fee.cal(amount, true);
    let total = amount + fee + service_fee;
    if user.balance < total {
        return Err(Error::Str("The balance is insufficient."));
    }

    let payee_inv = invoice::Entity::find()
        .filter(invoice::Column::PaymentHash.eq(payment_hash.clone()))
        .filter(invoice::Column::Type.eq(invoice::Type::Invoice))
        .one(conn)
        .await?
        .ok_or(Error::Str("Can't find payee invoice"))?;

    if payee_inv.status != invoice::Status::Unpaid {
        return Err(Error::Str("The invoice is closed."));
    }

    let time = now();
    let mut payment_model =
        create_invoice_active_model(&user, payee_inv.payment_preimage.clone(), inv);
    // payment
    payment_model.r#type = Set(invoice::Type::Payment);
    payment_model.lock_amount = Set(0);
    payment_model.status = Set(invoice::Status::Paid);
    payment_model.amount = Set(amount);
    payment_model.paid_amount = Set(amount);
    payment_model.fee = Set(fee);
    payment_model.total = Set(total);
    payment_model.paid_at = Set(time);
    payment_model.service_fee = Set(service_fee);
    payment_model.internal = Set(true);

    let payee_update = invoice::ActiveModel {
        status: Set(invoice::Status::Paid),
        amount: Set(amount),
        paid_amount: Set(amount),
        paid_at: Set(time),
        internal: Set(true),
        ..Default::default()
    };

    // exec pay
    let txn = conn.begin().await?;
    // update payee invoice status
    let res = invoice::Entity::update_many()
        .set(payee_update)
        .filter(invoice::Column::Id.eq(payee_inv.id))
        .filter(invoice::Column::Status.eq(invoice::Status::Unpaid))
        .exec(&txn)
        .await?;
    if res.rows_affected != 1 {
        return Err(Error::Str(
            "Update invoice failed, It's probably already been paid.",
        ));
    }

    let payment = payment_model.insert(&txn).await?;

    // Decrease payer balances
    let res = user::Entity::update_many()
        .col_expr(
            user::Column::Balance,
            Expr::col(user::Column::Balance).sub(total),
        )
        .filter(user::Column::Id.eq(user.id))
        .filter(user::Column::Balance.gte(total))
        .exec(&txn)
        .await?;
    if res.rows_affected != 1 {
        return Err(Error::Str("The balance is insufficient or locked."));
    }

    // Increase payee balance

    let res = user::Entity::update_many()
        .col_expr(
            user::Column::Balance,
            Expr::col(user::Column::Balance).add(amount),
        )
        .filter(user::Column::Id.eq(payee_inv.user_id))
        .exec(&txn)
        .await?;
    if res.rows_affected != 1 {
        return Err(Error::Str("unknown error. where is the user?"));
    }
    txn.commit().await?;

    Ok(payment)
}

async fn pay_failed(conn: &DbConn, model: invoice::Model) -> Result<()> {
    let lock_amount = model.lock_amount;

    let update = invoice::ActiveModel {
        status: Set(invoice::Status::Canceled),
        lock_amount: Set(0),
        ..Default::default()
    };

    let txn = conn.begin().await?;

    let res = invoice::Entity::update_many()
        .set(update)
        .filter(invoice::Column::Id.eq(model.id))
        .filter(invoice::Column::Status.eq(invoice::Status::Unpaid))
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
            .filter(user::Column::Id.eq(model.user_id))
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
    model: invoice::Model,
) -> Result<invoice::Model> {
    let lock_amount = model.lock_amount;
    let payback = lock_amount - model.service_fee - payment.total;

    let update = invoice::ActiveModel {
        payment_preimage: Set(payment.payment_preimage),
        status: Set(invoice::Status::Paid),
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
        .filter(invoice::Column::Id.eq(model.id))
        .filter(invoice::Column::LockAmount.eq(lock_amount))
        .exec(&txn)
        .await?;
    // check had updated
    if res.rows_affected == 1 {
        // update user lock balance
        let _res = user::Entity::update_many()
            .col_expr(
                user::Column::Balance,
                Expr::col(user::Column::Balance).add(payback),
            )
            .col_expr(
                user::Column::LockAmount,
                Expr::col(user::Column::LockAmount).sub(lock_amount),
            )
            .filter(user::Column::Pubkey.eq(model.user_pubkey.clone()))
            .filter(user::Column::LockAmount.gte(lock_amount))
            .exec(&txn)
            .await?;
        // if res.rows_affected != 1 {
        // }
    }
    txn.commit().await?;

    invoice::Entity::find_by_id(model.id)
        .one(conn)
        .await?
        .ok_or(Error::Str("where is the invoice?"))
}

fn create_invoice_active_model(
    user: &user::Model,
    preimage: Vec<u8>,
    invoice: lightning::Invoice,
) -> invoice::ActiveModel {
    invoice::ActiveModel {
        id: NotSet,
        user_id: Set(user.id),
        user_pubkey: Set(user.pubkey.clone()),
        payee: Set(invoice.payee),
        r#type: Set(invoice::Type::Invoice),
        status: Set(invoice::Status::Unpaid),
        payment_hash: Set(invoice.payment_hash),
        payment_preimage: Set(preimage),
        created_at: Set(invoice.created_at),
        expiry: Set(invoice.expiry),
        expired_at: Set(invoice.created_at + invoice.expiry),
        description: Set(invoice.description),
        bolt11: Set(invoice.bolt11),
        amount: Set(invoice.amount),
        paid_at: Set(0),
        paid_amount: Set(invoice.amount),
        fee: Set(0),
        total: Set(invoice.amount),
        lock_amount: Set(0),
        internal: Set(false),
        duplicate: Set(false),
        service_fee: Set(0),
    }
}
