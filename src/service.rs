use crate::{key::Pubkey, now, setting::Fee, sha256, Error, Result};
use entity::{donation, event, invoice, record, user};
use lightning_client::{lightning, Lightning};
use nostr_sdk::{secp256k1::XOnlyPublicKey, Event, EventId};
use rand::RngCore;
use sea_orm::{
    sea_query::Expr, ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbConn,
    EntityTrait, NotSet, QueryFilter, QueryOrder, Set, TransactionTrait,
};
use serde::Deserialize;
use std::{collections::HashMap, time::Duration};
use tokio::time::sleep;

pub fn rand_preimage() -> Vec<u8> {
    let mut store_key_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut store_key_bytes);
    store_key_bytes.to_vec()
}

#[derive(Deserialize, Debug, Default)]
pub struct InvoicePayer {
    pub name: Option<String>,
    pub email: Option<String>,
    pub pubkey: Option<Pubkey>,
}

#[derive(Deserialize, Debug, Default)]
pub struct InvoiceZap {
    pub from: Option<XOnlyPublicKey>,
    // zap to user pubkey (tag p)
    pub pubkey: Option<XOnlyPublicKey>,
    // zap to event id (tag e)
    pub event: Option<EventId>,
}

#[derive(Default)]
pub struct InvoiceExtra {
    pub source: invoice::Source,
    pub comment: Option<String>,
    pub zap: Option<InvoiceZap>,
    pub payer_data: Option<String>,
    pub payer: Option<InvoicePayer>,
}

impl InvoiceExtra {
    pub fn new(source: invoice::Source) -> Self {
        Self {
            source,
            ..Default::default()
        }
    }
}

type Lt = Box<dyn Lightning + Sync + Send>;
/// Lightning service
#[derive(Clone)]
pub struct Service {
    lightning: Lt,
    conn: DbConn,
    name: String,
    pub self_payment: bool,
    pub donation_receiver: Option<Vec<u8>>,
}

impl Service {
    pub fn new(name: String, lightning: Lt, conn: DbConn) -> Self {
        Self {
            name,
            lightning,
            conn,
            self_payment: false,
            donation_receiver: None,
        }
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    pub fn lightning(&self) -> &Lt {
        &self.lightning
    }

    pub fn db(&self) -> &DbConn {
        &self.conn
    }

    pub async fn info(&self) -> Result<lightning::Info> {
        Ok(self.lightning.get_info().await?)
    }

    pub async fn get_user_by_id(&self, id: i32) -> Result<user::Model> {
        get_user_by_id(self.db(), id).await
    }

    pub async fn get_user_by_name(&self, name: String) -> Result<Option<user::Model>> {
        Ok(user::Entity::find()
            .filter(user::Column::Username.eq(name))
            .one(self.db())
            .await?)
    }

    pub async fn get_user(&self, pubkey: Vec<u8>) -> Result<Option<user::Model>> {
        get_user(self.db(), pubkey).await
    }

    pub async fn admin_adjust_user_balance(
        &self,
        user: &user::Model,
        change: i64,
        note: Option<String>,
    ) -> Result<user::Model> {
        let txn = self.db().begin().await?;

        // update user balance
        let res = user::Entity::update_many()
            .col_expr(
                user::Column::Balance,
                Expr::col(user::Column::Balance).add(change),
            )
            .filter(user::Column::Id.eq(user.id))
            .exec(&txn)
            .await?;

        if res.rows_affected != 1 {
            return Err(Error::Str("update user balance error"));
        }

        new_record(user, None, change, "admin".to_owned(), note)
            .insert(&txn)
            .await?;
        txn.commit().await?;
        get_user_by_id(self.db(), user.id).await
    }

    pub async fn update_user_password(
        &self,
        user_id: i32,
        password: Option<String>,
    ) -> Result<user::Model> {
        Ok(user::ActiveModel {
            id: Set(user_id),
            password: Set(password),
            ..Default::default()
        }
        .update(self.db())
        .await?)
    }

    pub async fn update_username(&self, user_id: i32, name: Option<String>) -> Result<user::Model> {
        Ok(user::ActiveModel {
            id: Set(user_id),
            username: Set(name),
            ..Default::default()
        }
        .update(self.db())
        .await?)
    }

    pub async fn get_or_create_user(&self, pubkey: Vec<u8>) -> Result<user::Model> {
        get_or_create_user(self.db(), pubkey).await
    }

    pub async fn create_user(&self, pubkey: Vec<u8>) -> Result<user::Model> {
        create_user(self.db(), pubkey).await
    }

    pub async fn get_invoice(&self, id: i32) -> Result<Option<invoice::Model>> {
        Ok(invoice::Entity::find_by_id(id).one(self.db()).await?)
    }

    pub async fn create_invoice(
        &self,
        user: &user::Model,
        memo: String,
        msats: u64,
        expiry: u64,
        extra: InvoiceExtra,
    ) -> Result<invoice::Model> {
        let preimage = rand_preimage();
        let hash = sha256(&preimage);
        let invoice = self
            .lightning
            .create_invoice(memo.clone(), msats, Some(preimage.clone()), Some(expiry))
            .await?;

        if invoice.payment_hash != hash {
            return Err(Error::Str("invalid payment hash"));
        }

        let model = create_invoice_active_model(user, preimage, invoice, self.name.clone(), extra);

        Ok(model.insert(self.db()).await?)
    }

    pub async fn pay(
        &self,
        user: &user::Model,
        bolt11: String,
        fee: &Fee,
        source: invoice::Source,
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
            self.internal_pay(user, inv, fee, source).await
        } else {
            // external payment
            let payment_hash = inv.payment_hash.clone();

            let amount = inv.amount as i64;
            let (max_fee, service_fee) = fee.cal(amount, false);
            let total = amount + max_fee + service_fee;
            if user.balance < total {
                return Err(Error::Str("The balance is insufficient."));
            }

            let mut invoice = create_invoice_active_model(
                user,
                vec![],
                inv,
                self.name.clone(),
                InvoiceExtra::new(source),
            );
            // payment
            invoice.r#type = Set(invoice::Type::Payment);
            invoice.total = Set(total);
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
            let model = invoice.insert(&txn).await.map_err(|e| {
                if matches!(
                    e.sql_err(),
                    Some(sea_orm::SqlErr::UniqueConstraintViolation(_))
                ) {
                    Error::InvalidPayment("The payment already exists.".to_owned())
                } else {
                    Error::InvalidPayment(e.to_string())
                }
            })?;
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
                            pay_success(self.db(), &p, &model).await
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
                            Err(Error::PaymentInProgress)
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

    async fn internal_pay(
        &self,
        user: &user::Model,
        inv: lightning::Invoice,
        fee: &Fee,
        source: invoice::Source,
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
            .one(self.db())
            .await?
            .ok_or(Error::InvalidPayment("Can't find payee invoice".to_owned()))?;

        if !self.self_payment && payee_inv.user_id == user.id {
            return Err(Error::InvalidPayment(
                "Not allowed to pay yourself.".to_owned(),
            ));
        }

        let payee_user = get_user_by_id(self.db(), payee_inv.user_id).await?;

        if payee_inv.status != invoice::Status::Unpaid {
            return Err(Error::InvalidPayment("The invoice is closed.".to_owned()));
        }

        let time = now() as i64;
        let mut payment_model = create_invoice_active_model(
            user,
            payee_inv.payment_preimage.clone(),
            inv,
            self.name().clone(),
            InvoiceExtra::new(source),
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
        let txn = self.db().begin().await?;
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

        // record user balance change
        new_record(
            user,
            Some(payment.id),
            -total,
            "internal_payment".to_owned(),
            None,
        )
        .insert(&txn)
        .await?;

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

        new_record(
            &payee_user,
            Some(payee_inv.id),
            amount,
            "internal_payment".to_owned(),
            None,
        )
        .insert(&txn)
        .await?;

        sync_donation(self, Some(user.pubkey.clone()), &txn, &payee_inv).await?;

        txn.commit().await?;

        Ok(payment)
    }

    pub async fn sync(&self, duration: Duration, invoice_expiry: Duration) -> Result<()> {
        let seconds = invoice_expiry.as_secs();
        tracing::info!("start task for sync invoices and payments");
        loop {
            let from_time = now() - seconds;
            let r = self.sync_invoices(from_time).await;
            tracing::trace!("sync invoices {:?}", r);
            let r = self.sync_payments(None).await;
            tracing::trace!("sync payments {:?}", r);
            sleep(duration).await;
        }
    }

    pub async fn sync_invoices(&self, from_time: u64) -> Result<usize> {
        // get invoices unpaid for update status
        // get paid for check duplicate pay by external and internal
        let invoices = invoice::Entity::find()
            .filter(invoice::Column::Type.eq(invoice::Type::Invoice))
            .filter(invoice::Column::Status.ne(invoice::Status::Canceled))
            .filter(invoice::Column::GeneratedAt.gte(from_time as i64))
            .order_by_asc(invoice::Column::GeneratedAt)
            .all(self.db())
            .await?;
        let mut updated = 0;

        if invoices.is_empty() {
            return Ok(updated);
        }
        let first = invoices.first().unwrap();

        let map = self
            .lightning
            .list_invoices(Some((from_time, first.index as u64)), None)
            .await?
            .into_iter()
            .map(|inv| (inv.payment_hash.clone(), inv))
            .collect::<HashMap<_, _>>();
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
                                // todo: log error
                                let _r = invoice_paid(self, invoice, remote).await;
                            }

                            lightning::InvoiceStatus::Canceled => {
                                updated += 1;
                                // expired
                                let _res = invoice::Entity::update_many()
                                    .set(invoice::ActiveModel {
                                        status: Set(invoice::Status::Canceled),
                                        ..Default::default()
                                    })
                                    .filter(invoice::Column::Id.eq(invoice.id))
                                    .filter(invoice::Column::Status.eq(invoice::Status::Unpaid))
                                    .exec(self.db())
                                    .await;
                                // if res.rows_affected != 1 {
                                //     // log err
                                // }
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
                            // todo: log error
                            let _r = invoice_dup_paid(self.db(), invoice, remote).await;
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
                                // todo: log error
                                let _r = pay_success(self.db(), remote, payment).await;
                            }
                            lightning::PaymentStatus::Failed => {
                                updated += 1;
                                // todo: log error
                                let _r = pay_failed(self.db(), payment).await;
                            }
                        }
                    }
                }
            }
        }

        Ok(updated)
    }

    /// events service
    pub async fn get_event(&self, event_id: Vec<u8>) -> Result<Option<event::Model>> {
        Ok(event::Entity::find()
            .filter(event::Column::EventId.eq(event_id))
            .one(self.db())
            .await?)
    }

    pub async fn create_event(&self, event: &Event) -> Result<Option<event::Model>> {
        let now = now() as i64;
        let res = event::ActiveModel {
            id: NotSet,
            event_id: Set(event.id.as_bytes().to_vec()),
            json: Set(event.as_json()),
            created_at: Set(now),
            updated_at: Set(now),
            status: Set(event::Status::Created),
            message: NotSet,
        }
        .insert(self.db())
        .await;
        match res {
            Ok(res) => Ok(Some(res)),
            Err(e) => {
                if matches!(
                    e.sql_err(),
                    Some(sea_orm::SqlErr::UniqueConstraintViolation(_))
                ) {
                    // return false unique constraint
                    return Ok(None);
                }
                Err(e.into())
            }
        }
    }

    pub async fn update_event_error(&self, id: i32, err: &Error) -> Result<()> {
        event::ActiveModel {
            id: Set(id),
            status: Set(event::Status::Failed),
            message: Set(err.to_string()),
            updated_at: Set(now() as i64),
            ..Default::default()
        }
        .update(self.db())
        .await?;
        Ok(())
    }

    pub async fn update_event_success(&self, id: i32, message: String) -> Result<()> {
        event::ActiveModel {
            id: Set(id),
            status: Set(event::Status::Succeeded),
            message: Set(message),
            updated_at: Set(now() as i64),
            ..Default::default()
        }
        .update(self.db())
        .await?;
        Ok(())
    }
}

async fn get_or_create_user<C: ConnectionTrait>(conn: &C, pubkey: Vec<u8>) -> Result<user::Model> {
    match get_user(conn, pubkey.clone()).await? {
        Some(u) => Ok(u),
        None => create_user(conn, pubkey.clone()).await,
    }
}

async fn get_user<C: ConnectionTrait>(conn: &C, pubkey: Vec<u8>) -> Result<Option<user::Model>> {
    Ok(user::Entity::find()
        .filter(user::Column::Pubkey.eq(pubkey))
        .one(conn)
        .await?)
}

async fn create_user<C: ConnectionTrait>(conn: &C, pubkey: Vec<u8>) -> Result<user::Model> {
    let now = now() as i64;
    // create ConnectionTrait
    Ok(user::ActiveModel {
        pubkey: Set(pubkey),
        id: NotSet,
        balance: NotSet,
        lock_amount: NotSet,
        username: NotSet,
        password: NotSet,
        donate_amount: NotSet,
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(conn)
    .await?)
}

async fn invoice_dup_paid(
    conn: &DbConn,
    invoice: &invoice::Model,
    remote: &lightning::Invoice,
) -> Result<()> {
    let amount = remote.paid_amount as i64;
    let user = get_user_by_id(conn, invoice.user_id).await?;

    let txn = conn.begin().await?;
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

    if res.rows_affected != 1 {
        return Err(Error::InvalidPayment("Update invoice failed".to_owned()));
    }

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
        return Err(Error::InvalidPayment(
            "Update user balance failed".to_owned(),
        ));
    }

    new_record(
        &user,
        Some(invoice.id),
        amount,
        "duplicate_payment".to_owned(),
        None,
    )
    .insert(&txn)
    .await?;

    txn.commit().await?;
    Ok(())
}

async fn invoice_paid(
    service: &Service,
    invoice: &invoice::Model,
    remote: &lightning::Invoice,
) -> Result<()> {
    let conn = service.db();
    let amount = remote.paid_amount as i64;

    let user = get_user_by_id(conn, invoice.user_id).await?;

    let txn = conn.begin().await?;
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

    if res.rows_affected != 1 {
        return Err(Error::InvalidPayment("Update invoice failed".to_owned()));
    }

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
        return Err(Error::InvalidPayment(
            "Update user balance failed".to_owned(),
        ));
    }

    new_record(
        &user,
        Some(invoice.id),
        amount,
        "external_payment".to_owned(),
        None,
    )
    .insert(&txn)
    .await?;

    sync_donation(service, None, &txn, invoice).await?;

    txn.commit().await?;
    Ok(())
}

// Get donation user from payer pubkey, internal user, zap
async fn sync_donation(
    service: &Service,
    user: Option<Vec<u8>>,
    txn: &DatabaseTransaction,
    invoice: &invoice::Model,
) -> Result<bool> {
    if Some(&invoice.user_pubkey) == service.donation_receiver.as_ref() {
        let mut pubkey = None;

        // try get user from payer data
        if let Some(data) = &invoice.payer_pubkey {
            pubkey = Some(data.clone());
        }

        // try get user from zap
        if pubkey.is_none() && invoice.zap {
            pubkey = invoice.zap_from.clone();
        }

        // internal user
        if pubkey.is_none() {
            pubkey = user;
        }
        if let Some(pubkey) = pubkey {
            let user = get_or_create_user(txn, pubkey.clone()).await?;
            let now = now() as i64;
            donation::ActiveModel {
                id: NotSet,
                user_id: Set(user.id),
                invoice_id: Set(invoice.id),
                status: Set(donation::Status::Paid),
                amount: Set(invoice.paid_amount),
                paid_at: Set(now),
                message: NotSet,
                created_at: Set(now),
                updated_at: Set(now),
            }
            .insert(txn)
            .await?;
            // update user donate amount
            user::Entity::update_many()
                .col_expr(user::Column::UpdatedAt, Expr::value(now))
                .col_expr(
                    user::Column::DonateAmount,
                    Expr::col(user::Column::DonateAmount).add(invoice.paid_amount),
                )
                .filter(user::Column::Id.eq(user.id))
                .exec(txn)
                .await?;
            return Ok(true);
        }
    }
    Ok(false)
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
    if res.rows_affected != 1 {
        return Err(Error::InvalidPayment("Update invoice failed".to_owned()));
    }

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

    if res.rows_affected != 1 {
        return Err(Error::InvalidPayment(
            "Update user balance failed".to_owned(),
        ));
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
    let total = lock_amount - payback;

    let update = invoice::ActiveModel {
        payment_preimage: Set(payment.payment_preimage.clone()),
        status: Set(invoice::Status::Paid),
        lock_amount: Set(0),
        amount: Set(payment.amount as i64),
        paid_amount: Set(payment.amount as i64),
        fee: Set(payment.fee as i64),
        total: Set(total),
        paid_at: Set(payment.created_at as i64),
        ..Default::default()
    };

    let user = get_user_by_id(conn, model.user_id).await?;

    let txn = conn.begin().await?;

    let res = invoice::Entity::update_many()
        .set(update)
        .filter(invoice::Column::Id.eq(model.id))
        .filter(invoice::Column::LockAmount.eq(lock_amount))
        .exec(&txn)
        .await?;
    // check had updated
    if res.rows_affected != 1 {
        return Err(Error::InvalidPayment("Update invoice failed".to_owned()));
    }
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
        .filter(user::Column::Id.eq(model.user_id))
        .filter(user::Column::LockAmount.gte(lock_amount))
        .exec(&txn)
        .await?;
    if res.rows_affected != 1 {
        return Err(Error::InvalidPayment(
            "Update user balance failed".to_owned(),
        ));
    }

    // record user balance change
    new_record(
        &user,
        Some(model.id),
        -total,
        "external_payment".to_owned(),
        None,
    )
    .insert(&txn)
    .await?;

    txn.commit().await?;

    invoice::Entity::find_by_id(model.id)
        .one(conn)
        .await?
        .ok_or(Error::Str("where is the invoice?"))
}

fn new_record(
    user: &user::Model,
    invoice_id: Option<i32>,
    change: i64,
    source: String,
    note: Option<String>,
) -> record::ActiveModel {
    let now = now();
    record::ActiveModel {
        id: NotSet,
        user_id: Set(user.id),
        invoice_id: Set(invoice_id),
        user_pubkey: Set(user.pubkey.clone()),
        balance: Set(user.balance),
        change: Set(change),
        source: Set(source),
        created_at: Set(now as i64),
        note: Set(note.unwrap_or_default()),
    }
}

async fn get_user_by_id(conn: &DbConn, id: i32) -> Result<user::Model> {
    user::Entity::find_by_id(id)
        .one(conn)
        .await?
        .ok_or(Error::Str("missing user"))
}

fn create_invoice_active_model(
    user: &user::Model,
    preimage: Vec<u8>,
    invoice: lightning::Invoice,
    service: String,
    extra: InvoiceExtra,
) -> invoice::ActiveModel {
    let now = now();
    let payer = extra.payer.unwrap_or_default();
    let is_zap = extra.zap.is_some();
    let zap = extra.zap.unwrap_or_default();

    invoice::ActiveModel {
        id: NotSet,
        user_id: Set(user.id),
        user_pubkey: Set(user.pubkey.clone()),
        payee: Set(invoice.payee),
        index: Set(invoice.index as i64),
        r#type: Set(invoice::Type::Invoice),
        status: Set(invoice::Status::Unpaid),
        payment_hash: Set(invoice.payment_hash),
        payment_preimage: Set(preimage),
        generated_at: Set(invoice.created_at as i64),
        expiry: Set(invoice.expiry as i64),
        expired_at: Set((invoice.created_at + invoice.expiry) as i64),
        description: Set(invoice.description.unwrap_or_default()),
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
        source: Set(extra.source),
        service: Set(service),
        created_at: Set(now as i64),
        updated_at: Set(now as i64),
        comment: Set(extra.comment),
        payer: Set(extra.payer_data),
        payer_name: Set(payer.name),
        payer_email: Set(payer.email),
        payer_pubkey: Set(payer.pubkey.map(|k| k.serialize().to_vec())),
        zap: Set(is_zap),
        zap_from: Set(zap.from.map(|k| k.serialize().to_vec())),
        zap_pubkey: Set(zap.pubkey.map(|k| k.serialize().to_vec())),
        zap_event: Set(zap.event.map(|e| e.as_bytes().to_vec())),
        zap_status: NotSet,
        zap_receipt: NotSet,
    }
}
