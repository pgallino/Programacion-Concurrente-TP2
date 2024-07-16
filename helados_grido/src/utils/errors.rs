use std::{
    any::Any,
    array::TryFromSliceError,
    fmt, io,
    sync::{
        mpsc::{self, SendError},
        MutexGuard, PoisonError, RwLockReadGuard, RwLockWriteGuard,
    },
};

#[derive(Debug)]
pub enum ParseError {
    EmptyPacket,
    IncompleteCapturePacket,
    IncompleteCommitPacket,
    IncompleteAbortPacket,
    UnknownPacket,
    ConversionError,
}

impl From<TryFromSliceError> for ParseError {
    fn from(_: TryFromSliceError) -> Self {
        ParseError::ConversionError
    }
}

#[derive(Debug)]
pub enum RobotError {
    Io(io::Error),
    Parse(ParseError),
    Channel(String),
    Lock(String),
    Handler(String),
}

impl fmt::Display for RobotError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RobotError::Io(error) => write!(f, "Hubo un error de salida: {error}"),
            RobotError::Parse(error) => write!(f, "Hubo un error de parseo: {:?}", error),
            RobotError::Channel(error_msg) => write!(f, "{error_msg}"),
            RobotError::Lock(error_msg) => write!(f, "{error_msg}"),
            RobotError::Handler(error_msg) => write!(f, "{error_msg}"),
        }
    }
}

impl From<ParseError> for RobotError {
    fn from(err: ParseError) -> Self {
        RobotError::Parse(err)
    }
}

impl From<io::Error> for RobotError {
    fn from(err: io::Error) -> Self {
        RobotError::Io(err)
    }
}

impl From<mpsc::RecvError> for RobotError {
    fn from(err: mpsc::RecvError) -> Self {
        RobotError::Channel(format!("Hubo un error al leer de un channel: {err}"))
    }
}

impl<T> From<SendError<T>> for RobotError {
    fn from(err: SendError<T>) -> RobotError {
        RobotError::Channel(format!("Hubo un error al escribir por un channel: {err}"))
    }
}

impl<T> From<PoisonError<MutexGuard<'_, T>>> for RobotError {
    fn from(err: PoisonError<MutexGuard<'_, T>>) -> Self {
        RobotError::Lock(format!("Hubo un error al hacer lock(): {err}"))
    }
}

impl<T> From<PoisonError<RwLockReadGuard<'_, T>>> for RobotError {
    fn from(err: PoisonError<RwLockReadGuard<'_, T>>) -> RobotError {
        RobotError::Lock(format!("Hubo un error al hacer lock de lectura: {err}"))
    }
}

impl<T> From<PoisonError<RwLockWriteGuard<'_, T>>> for RobotError {
    fn from(err: PoisonError<RwLockWriteGuard<'_, T>>) -> RobotError {
        RobotError::Lock(format!(
            "Hubo un error al hacer un lock de escritura: {err}"
        ))
    }
}

#[derive(Debug)]
pub enum ScreenError {
    Io(io::Error),
    Parse(ParseError),
    Channel(String),
    Lock(String),
}

impl fmt::Display for ScreenError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ScreenError::Io(error) => write!(f, "Hubo un error de salida: {error}"),
            ScreenError::Parse(error) => write!(f, "Hubo un error de parseo: {:?}", error),
            ScreenError::Channel(error_msg) => write!(f, "{error_msg}"),
            ScreenError::Lock(error_msg) => write!(f, "{error_msg}"),
        }
    }
}

impl From<ParseError> for ScreenError {
    fn from(err: ParseError) -> Self {
        ScreenError::Parse(err)
    }
}

impl From<io::Error> for ScreenError {
    fn from(err: io::Error) -> Self {
        ScreenError::Io(err)
    }
}

impl From<mpsc::RecvError> for ScreenError {
    fn from(err: mpsc::RecvError) -> Self {
        ScreenError::Channel(format!("Hubo un error al leer de un channel: {err}"))
    }
}

impl<T> From<SendError<T>> for ScreenError {
    fn from(err: SendError<T>) -> ScreenError {
        ScreenError::Channel(format!("Hubo un error al escribir por un channel: {err}"))
    }
}

impl<T> From<PoisonError<MutexGuard<'_, T>>> for ScreenError {
    fn from(err: PoisonError<MutexGuard<'_, T>>) -> Self {
        ScreenError::Lock(format!("Hubo un error al hacer lock(): {err}"))
    }
}

impl<T> From<PoisonError<RwLockReadGuard<'_, T>>> for ScreenError {
    fn from(err: PoisonError<RwLockReadGuard<'_, T>>) -> ScreenError {
        ScreenError::Lock(format!("Hubo un error al hacer lock de lectura: {err}"))
    }
}

impl<T> From<PoisonError<RwLockWriteGuard<'_, T>>> for ScreenError {
    fn from(err: PoisonError<RwLockWriteGuard<'_, T>>) -> ScreenError {
        ScreenError::Lock(format!(
            "Hubo un error al hacer un lock de escritura: {err}"
        ))
    }
}

impl<T> From<actix::prelude::SendError<T>> for ScreenError {
    fn from(err: actix::prelude::SendError<T>) -> Self {
        ScreenError::Channel(format!("Hubo un error al leer de un channel: {err}"))
    }
}

impl From<Box<dyn Any + Send>> for RobotError {
    fn from(err: Box<dyn Any + Send>) -> Self {
        RobotError::Handler(format!("Hubo un error al handlear el thread: {:?}", err))
    }
}
