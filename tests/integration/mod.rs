mod constants;
mod utils;

#[cfg(feature = "blocking")]
mod blocking;

#[cfg(feature = "async")]
mod nonblocking;
