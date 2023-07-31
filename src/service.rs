use crate::{now, setting::Fee, Error, Result};
use entity::{invoice, user};
use lightning_client::{lightning, Lightning};
use rand::RngCore;
use sea_orm::{
    sea_query::Expr, ActiveModelTrait, ColumnTrait, DbConn, EntityTrait, NotSet, QueryFilter,
    QueryOrder, Set, TransactionTrait,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub fn rand_preimage() -> Vec<u8> {
    let mut store_key_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut store_key_bytes);
    store_key_bytes.to_vec()
}

/// Lightning service
// #[derive(Clone)]
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

    pub fn lightning(&self) -> &Box<dyn Lightning + Sync + Send> {
        &self.lightning
    }

    pub fn db(&self) -> &DbConn {
        &self.conn
    }

    pub async fn info(&self) -> Result<lightning::Info> {
        Ok(self.lightning.get_info().await?)
    }

    pub async fn get_user(&self, pubkey: Vec<u8>) -> Result<Option<user::Model>> {
        Ok(user::Entity::find()
            .filter(user::Column::Pubkey.eq(pubkey))
            .one(self.db())
            .await?)
    }

    pub async fn update_user_balance(
        &self,
        user: &user::Model,
        balance: i64,
    ) -> Result<user::Model> {
        Ok(user::ActiveModel {
            id: Set(user.id),
            balance: Set(balance),
            ..Default::default()
        }
        .update(self.db())
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
                .insert(self.db())
                .await?)
            }
        }
    }

    pub async fn get_invoice(&self, id: i64) -> Result<Option<invoice::Model>> {
        Ok(invoice::Entity::find_by_id(id).one(self.db()).await?)
    }

    pub async fn create_invoice(
        &self,
        user: &user::Model,
        memo: String,
        msats: u64,
        expiry: u64,
        source: String,
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

        let model = create_invoice_active_model(user, preimage, invoice, self.name.clone(), source);

        Ok(model.insert(self.db()).await?)
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
        // expired
        if inv.created_at + inv.expiry <= now() {
            return Err(Error::Str("The invoice is expired."));
        }
        if info.id.eq(&inv.payee) {
            // internal payment
            internal_pay(&self.conn, user, inv, fee, self.name.clone()).await
        } else {
            // external payment
            let payment_hash = inv.payment_hash.clone();

            let amount = inv.amount as i64;
            let (max_fee, service_fee) = fee.cal(amount, false);
            let total = amount + max_fee + service_fee;
            if user.balance < total {
                return Err(Error::Str("The balance is insufficient."));
            }

            let mut invoice =
                create_invoice_active_model(&user, vec![], inv, self.name.clone(), "".to_owned());
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
                return Err(Error::InvalidPayment(
                    "The balance is insufficient or locked.".to_owned(),
                ));
            }

            // create payment
            let model = invoice
                .insert(&txn)
                .await
                .map_err(|e| Error::InvalidPayment(e.to_string()))?;
            txn.commit().await?;

            // try pay
            let pay = self.lightning.pay(bolt11, Some(max_fee as u64)).await;

            // don't check payment result
            if ignore_result {
                return Ok(model);
            }

            let payment = self.lightning.lookup_payment(payment_hash).await;

            match payment {
                Ok(p) => {
                    match p.status {
                        lightning::PaymentStatus::Succeeded => {
                            pay_success(&self.db(), &p, &model).await
                        }
                        lightning::PaymentStatus::Failed => {
                            // failed
                            pay_failed(self.db(), &model).await?;
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
                    pay_failed(self.db(), &model).await?;
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

    pub async fn sync_invoices(&self, from_time: u64) -> Result<usize> {
        // get invoices unpaid for update status
        // get paid for check duplicate pay by external and internal
        let invoices = invoice::Entity::find()
            .filter(invoice::Column::Type.eq(invoice::Type::Invoice))
            .filter(invoice::Column::Status.ne(invoice::Status::Canceled))
            .filter(invoice::Column::GeneratedAt.gte(from_time as i64))
            .all(self.db())
            .await?;
        let map = self
            .lightning
            .list_invoices(Some(from_time), None)
            .await?
            .into_iter()
            .map(|inv| (inv.payment_hash.clone(), inv))
            .collect::<HashMap<_, _>>();
        let mut updated = 0;
        for invoice in invoices.iter() {
            if let Some(remote) = map.get(&invoice.payment_hash) {
                match invoice.status {
                    invoice::Status::Unpaid => {
                        match remote.status {
                            lightning::InvoiceStatus::Open => {
                                // ignore
                            }
                            lightning::InvoiceStatus::Paid => {
                                // updated paid
                                updated += 1;
                                let amount = remote.paid_amount as i64;

                                let txn = self.db().begin().await?;
                                // update payee invoice status
                                let res = invoice::Entity::update_many()
                                    .set(invoice::ActiveModel {
                                        status: Set(invoice::Status::Paid),
                                        paid_amount: Set(amount),
                                        paid_at: Set(remote.paid_at as i64),
                                        internal: Set(false),
                                        ..Default::default()
                                    })
                                    .filter(invoice::Column::Id.eq(invoice.id))
                                    .filter(invoice::Column::Status.eq(invoice::Status::Unpaid))
                                    .exec(&txn)
                                    .await?;

                                if res.rows_affected == 1 {
                                    // update user balance
                                    let res = user::Entity::update_many()
                                        .col_expr(
                                            user::Column::Balance,
                                            Expr::col(user::Column::Balance).add(amount),
                                        )
                                        .filter(user::Column::Id.eq(invoice.user_id))
                                        .exec(&txn)
                                        .await?;
                                    if res.rows_affected != 1 {
                                        // log err
                                    }
                                } else {
                                    // log err
                                }
                                txn.commit().await?;
                            }
                            lightning::InvoiceStatus::Canceled => {
                                updated += 1;
                                // expired
                                let res = invoice::Entity::update_many()
                                    .set(invoice::ActiveModel {
                                        status: Set(invoice::Status::Canceled),
                                        ..Default::default()
                                    })
                                    .filter(invoice::Column::Id.eq(invoice.id))
                                    .filter(invoice::Column::Status.eq(invoice::Status::Unpaid))
                                    .exec(self.db())
                                    .await?;
                                if res.rows_affected != 1 {
                                    // log err
                                }
                            }
                        }
                    }
                    invoice::Status::Paid => {
                        // check duplicate pay by external and internal
                        if remote.status == lightning::InvoiceStatus::Paid
                            && invoice.status == invoice::Status::Paid
                            && invoice.internal
                            && !invoice.duplicate
                        {
                            updated += 1;
                            let amount = remote.paid_amount as i64;

                            let txn = self.db().begin().await?;
                            // update payee invoice status
                            let res = invoice::Entity::update_many()
                                .set(invoice::ActiveModel {
                                    paid_amount: Set(amount + invoice.paid_amount),
                                    duplicate: Set(true),
                                    ..Default::default()
                                })
                                .filter(invoice::Column::Id.eq(invoice.id))
                                .filter(invoice::Column::Status.eq(invoice::Status::Paid))
                                .filter(invoice::Column::Internal.eq(true))
                                .filter(invoice::Column::Duplicate.eq(false))
                                .exec(&txn)
                                .await?;

                            if res.rows_affected == 1 {
                                // update user balance
                                let res = user::Entity::update_many()
                                    .col_expr(
                                        user::Column::Balance,
                                        Expr::col(user::Column::Balance).add(amount),
                                    )
                                    .filter(user::Column::Id.eq(invoice.user_id))
                                    .exec(&txn)
                                    .await?;
                                if res.rows_affected != 1 {
                                    // log err
                                }
                            } else {
                                // log err
                            }
                            txn.commit().await?;
                        }
                    }
                    invoice::Status::Canceled => {
                        // expired
                    }
                }
            }
        }
        Ok(updated)
    }

    pub async fn sync_payments(&self, from_time: Option<u64>) -> Result<usize> {
        let payments = invoice::Entity::find()
            .filter(invoice::Column::Type.eq(invoice::Type::Payment))
            .filter(invoice::Column::Status.eq(invoice::Status::Unpaid))
            .order_by_asc(invoice::Column::GeneratedAt)
            .all(self.db())
            .await?;

        let mut updated = 0;

        if !payments.is_empty() {
            let from_time = from_time.unwrap_or(payments[0].generated_at as u64);
            let map = self
                .lightning
                .list_payments(Some(from_time), None)
                .await?
                .into_iter()
                .map(|inv| (inv.payment_hash.clone(), inv))
                .collect::<HashMap<_, _>>();
            for payment in payments.iter() {
                if let Some(remote) = map.get(&payment.payment_hash) {
                    if payment.status == invoice::Status::Unpaid {
                        match remote.status {
                            lightning::PaymentStatus::Unknown => {}
                            lightning::PaymentStatus::InFlight => {}
                            lightning::PaymentStatus::Succeeded => {
                                updated += 1;
                                pay_success(self.db(), remote, payment).await?;
                            }
                            lightning::PaymentStatus::Failed => {
                                updated += 1;
                                pay_failed(self.db(), payment).await?;
                            }
                        }
                    }
                }
            }
        }

        Ok(updated)
    }
}

async fn internal_pay(
    conn: &DbConn,
    user: &user::Model,
    inv: lightning::Invoice,
    fee: &Fee,
    service: String,
) -> Result<invoice::Model> {
    let payment_hash = inv.payment_hash.clone();
    let amount = inv.amount as i64;
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
        .ok_or(Error::InvalidPayment("Can't find payee invoice".to_owned()))?;

    if payee_inv.status != invoice::Status::Unpaid {
        return Err(Error::InvalidPayment("The invoice is closed.".to_owned()));
    }

    let time = now() as i64;
    let mut payment_model = create_invoice_active_model(
        &user,
        payee_inv.payment_preimage.clone(),
        inv,
        service,
        "".to_owned(),
    );
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
        return Err(Error::InvalidPayment(
            "Update invoice failed, It's probably already been paid.".to_owned(),
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

async fn pay_failed(conn: &DbConn, model: &invoice::Model) -> Result<()> {
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
    payment: &lightning::Payment,
    model: &invoice::Model,
) -> Result<invoice::Model> {
    let lock_amount = model.lock_amount;
    let payback = lock_amount - model.service_fee - payment.total as i64;

    let update = invoice::ActiveModel {
        payment_preimage: Set(payment.payment_preimage.clone()),
        status: Set(invoice::Status::Paid),
        lock_amount: Set(0),
        amount: Set(payment.amount as i64),
        paid_amount: Set(payment.amount as i64),
        fee: Set(payment.fee as i64),
        total: Set(payment.total as i64),
        paid_at: Set(payment.created_at as i64),
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
    service: String,
    source: String,
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
        generated_at: Set(invoice.created_at as i64),
        expiry: Set(invoice.expiry as i64),
        expired_at: Set((invoice.created_at + invoice.expiry) as i64),
        description: Set(invoice.description),
        bolt11: Set(invoice.bolt11),
        amount: Set(invoice.amount as i64),
        paid_at: Set(0),
        paid_amount: Set(invoice.amount as i64),
        fee: Set(0),
        total: Set(invoice.amount as i64),
        lock_amount: Set(0),
        internal: Set(false),
        duplicate: Set(false),
        service_fee: Set(0),
        source: Set(source),
        service: Set(service),
        created_at: Set(now() as i64),
    }
}
