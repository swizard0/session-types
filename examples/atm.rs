extern crate session_types_ng;

use std::thread::spawn;
use std::sync::mpsc::{SendError, RecvError};

use session_types_ng::*;
use session_types_ng::mpsc::Value;

type Id = String;
type Atm = Recv<Value<Id>, Choose<Rec<AtmInner>, Choose<End, Nil>>>;

type AtmInner =
    Offer<AtmDeposit,
    Offer<AtmWithdraw,
    Offer<AtmBalance,
    Offer<End, Nil>>>>;

type AtmDeposit = Recv<Value<u64>, Send<Value<u64>, Var<Z>>>;
type AtmWithdraw = Recv<Value<u64>, Choose<Var<Z>, Choose<Var<Z>, Nil>>>;
type AtmBalance = Send<Value<u64>, Var<Z>>;

type Client = <Atm as HasDual>::Dual;

fn approved(id: &Id) -> bool {
    !id.is_empty()
}

type SendChoiceError = SendError<Box<bool>>;
type RecvChoiceError = RecvError;
type SendAmountError = SendError<Box<u64>>;
type RecvOfferError = RecvError;
type SendIdError = SendError<Box<Id>>;

#[derive(Debug)]
enum AtmError {
    RecvId(RecvError),
    FailChooseId(SendChoiceError),
    SuccessChooseId(SendChoiceError),
    RecvDeposit(RecvError),
    SendDepositBalance(SendAmountError),
    RecvWithdraw(RecvError),
    FailChooseWithdraw(SendChoiceError),
    SuccessChooseWithdraw(SendChoiceError),
    SendBalance(SendAmountError),
    OfferAtm(RecvOfferError),
}

fn atm(chan: Chan<mpsc::Channel, (), Atm>) -> Result<(), AtmError> {
    let mut chan = {
        let (chan, Value(id)) = chan.recv().map_err(AtmError::RecvId)?;
        if !approved(&id) {
            chan.second().map_err(AtmError::FailChooseId)?.close();
            return Ok(());
        }
        chan.first().map_err(AtmError::SuccessChooseId)?.enter()
    };

    let mut balance = 0;
    loop {
        enum Req<D, W, B, Q> {
            Deposit(D),
            Withdraw(W),
            Balance(B),
            Quit(Q),
        }

        let req = chan
            .offer()
            .option(Req::Deposit)
            .option(Req::Withdraw)
            .option(Req::Balance)
            .option(Req::Quit)
            .map_err(AtmError::OfferAtm)?;

        match req {
            Req::Deposit(chan_deposit) => {
                let (c, Value(amt)) = chan_deposit.recv().map_err(AtmError::RecvDeposit)?;
                balance += amt;
                chan = c.send(Value(balance)).map_err(AtmError::SendDepositBalance)?.zero();
            },
            Req::Withdraw(chan_withdraw) => {
                let (c, Value(amt)) = chan_withdraw.recv().map_err(AtmError::RecvWithdraw)?;
                chan =
                    if amt > balance {
                        c.second().map_err(AtmError::FailChooseWithdraw)?.zero()
                    } else {
                        balance -= amt;
                        c.first().map_err(AtmError::SuccessChooseWithdraw)?.zero()
                    };
            },
            Req::Balance(chan_balance) =>
                chan = chan_balance.send(Value(balance)).map_err(AtmError::SendBalance)?.zero(),
            Req::Quit(chan_quit) => {
                chan_quit.close();
                return Ok(());
            },
        }
    }
}

#[derive(Debug)]
enum ClientError {
    SendId(SendIdError),
    LoginFailed(&'static str),
    OfferClient(RecvOfferError),
    FailChooseDeposit(SendChoiceError),
    SendDeposit(SendAmountError),
    RecvBalance(RecvError),
    FailChooseQuit(SendChoiceError),
    FailChooseWithdraw(SendChoiceError),
    SendWithdraw(SendAmountError),
}

fn login_client(chan: Chan<mpsc::Channel, (), Client>, login: &str) ->
    Result<Chan<mpsc::Channel, (<AtmInner as HasDual>::Dual, ()), <AtmInner as HasDual>::Dual>, ClientError>
{
    let chan = chan
        .send(Value(login.to_string())).map_err(ClientError::SendId)?
        .offer()
        .option(|chan_success| Ok(chan_success.enter()))
        .option(|chan_fail| {
            chan_fail.close();
            Err(ClientError::LoginFailed("expected to be approved"))
        })
        .map_err(ClientError::OfferClient)??;
    Ok(chan)
}

fn deposit_client(chan: Chan<mpsc::Channel, (), Client>) -> Result<(), ClientError> {
    let chan = login_client(chan, "Deposit Client")?;
    let (chan, Value(new_balance)) = chan
        .first().map_err(ClientError::FailChooseDeposit)?
        .send(Value(200)).map_err(ClientError::SendDeposit)?
        .recv().map_err(ClientError::RecvBalance)?;
    println!("deposit_client: new balance: {}", new_balance);
    chan.zero()
        .fourth().map_err(ClientError::FailChooseQuit)?
        .close();
    Ok(())
}

fn withdraw_client(chan: Chan<mpsc::Channel, (), Client>) -> Result<(), ClientError> {
    login_client(chan, "Withdraw Client")?
        .second().map_err(ClientError::FailChooseWithdraw)?
        .send(Value(100)).map_err(ClientError::SendWithdraw)?
        .offer()
        .option(|chan_success| {
            println!("withdraw_client: successfully withdrew 100");
            chan_success
                .zero()
                .fourth().map_err(ClientError::FailChooseQuit)?
                .close();
            Ok(())
        })
        .option(|chan_fail| {
            println!("withdraw_client: could not withdraw. Depositing instead.");
            chan_fail
                .zero()
                .first().map_err(ClientError::FailChooseDeposit)?
                .send(Value(50)).map_err(ClientError::SendDeposit)?
                .recv().map_err(ClientError::RecvBalance)?
                .0
                .zero()
                .fourth().map_err(ClientError::FailChooseQuit)?
                .close();
            Ok(())
        })
        .map_err(ClientError::OfferClient)?
}

fn main() {
    let (atm_chan, client_chan) = mpsc::session_channel();
    spawn(|| atm(atm_chan).unwrap());
    deposit_client(client_chan).unwrap();

    let (atm_chan, client_chan) = mpsc::session_channel();
    spawn(|| atm(atm_chan).unwrap());
    withdraw_client(client_chan).unwrap();
}
