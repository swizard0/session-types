extern crate session_types_ng;

use std::thread::spawn;
use std::sync::mpsc::{SendError, RecvError};

use session_types_ng::*;
use session_types_ng::mpsc::Value;

type Id = String;
type Atm = Recv<Value<Id>, Choose<Rec<AtmInner>, More<Choose<End, Nil>>>>;

type AtmInner =
    Offer<AtmDeposit, More<
    Offer<AtmWithdraw, More<
    Offer<AtmBalance, More<
    Offer<End, Nil>>>>>>>;

type AtmDeposit = Recv<Value<u64>, Send<Value<u64>, Var<Z>>>;
type AtmWithdraw = Recv<Value<u64>, Choose<Var<Z>, More<Choose<Var<Z>, Nil>>>>;
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
            chan
                .tail().map_err(AtmError::FailChooseId)?
                .head().map_err(AtmError::FailChooseId)?
                .close();
            return Ok(());
        }
        chan.head().map_err(AtmError::SuccessChooseId)?.enter()
    };

    let mut balance = 0;
    loop {
        enum Action {
            Stop,
            Next(Chan<mpsc::Channel, (AtmInner, ()), AtmInner>),
        }

        let action = chan
            .offer()
            .option(|chan_deposit| {
                let (chan, Value(amt)) = chan_deposit.recv().map_err(AtmError::RecvDeposit)?;
                balance += amt;
                let chan = chan.send(Value(balance)).map_err(AtmError::SendDepositBalance)?.zero();
                Ok(Action::Next(chan))
            })
            .option(|chan_withdraw| {
                let (chan, Value(amt)) = chan_withdraw.recv().map_err(AtmError::RecvWithdraw)?;
                let chan =
                    if amt > balance {
                        chan
                            .tail().map_err(AtmError::FailChooseWithdraw)?
                            .head().map_err(AtmError::FailChooseWithdraw)?
                            .zero()
                    } else {
                        balance -= amt;
                        chan.head().map_err(AtmError::SuccessChooseWithdraw)?.zero()
                    };
                Ok(Action::Next(chan))
            })
            .option(|chan_balance| {
                let chan = chan_balance.send(Value(balance)).map_err(AtmError::SendBalance)?.zero();
                Ok(Action::Next(chan))
            })
            .option(|chan_quit| {
                chan_quit.close();
                Ok(Action::Stop)
            })
            .map_err(AtmError::OfferAtm)??;

        match action {
            Action::Stop =>
                return Ok(()),
            Action::Next(next_chan) =>
                chan = next_chan,
        };
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
        .head().map_err(ClientError::FailChooseDeposit)?
        .send(Value(200)).map_err(ClientError::SendDeposit)?
        .recv().map_err(ClientError::RecvBalance)?;
    println!("deposit_client: new balance: {}", new_balance);
    chan.zero()
        .skip3().map_err(ClientError::FailChooseQuit)?
        .head().map_err(ClientError::FailChooseQuit)?
        .close();
    Ok(())
}

fn withdraw_client(chan: Chan<mpsc::Channel, (), Client>) -> Result<(), ClientError> {
    login_client(chan, "Withdraw Client")?
        .tail().map_err(ClientError::FailChooseWithdraw)?
        .head().map_err(ClientError::FailChooseWithdraw)?
        .send(Value(100)).map_err(ClientError::SendWithdraw)?
        .offer()
        .option(|chan_success| {
            println!("withdraw_client: successfully withdrew 100");
            chan_success
                .zero()
                .skip3().map_err(ClientError::FailChooseQuit)?
                .head().map_err(ClientError::FailChooseQuit)?
                .close();
            Ok(())
        })
        .option(|chan_fail| {
            println!("withdraw_client: could not withdraw. Depositing instead.");
            chan_fail
                .zero()
                .head().map_err(ClientError::FailChooseDeposit)?
                .send(Value(50)).map_err(ClientError::SendDeposit)?
                .recv().map_err(ClientError::RecvBalance)?
                .0
                .zero()
                .skip3().map_err(ClientError::FailChooseQuit)?
                .head().map_err(ClientError::FailChooseQuit)?
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
