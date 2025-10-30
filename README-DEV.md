# Development

## Layout

This project is a little unusual in that it uses [sync_executor](https://crates.io/crates/sync_executor) to make it easy to support both a blocking and async interface, built from the same set of async code. You can read the details there, but the TL;DR is that this project is basically an "upside down" sans-io project:

- The `maybe_async` module defines a bunch of traits. These traits have async functions that hide all the "blocking" tasks we might want to do; writing files, network requests, sleeping, etc...
- Code in the `shared` module all looks like async code, but async functions in the shared module may only call into other async functions in the shared module, or into functions in the `maybe_async` traits. They may not call directly into async functions (like tokio or reqwest). This module exists to get rust to build a "state machine" for us (which is why I say this is like an upside down sans-io project).
- The `blocking::imp` and `nonblocking::imp` modules have various concrete implementations of the `maybe_async` traits. The nonblocking implementations make calls into async libraries (such as `feat::reqwest_client`) and the blocking ones make calls into blocking libraries (such as `feat::ureq_client`).
- The `blocking` and `nonblocking` modules are the public API to this crate, which instantiate concrete instances from the `imp` crates and pass them down into the `shared` module.  The `nonblocking` version is pretty straightforward.  The magic in the `blocking` one comes the fact that because our `blocking::imp` implementations only ever call into blocking code, we know that any async function in `shared` that relies on them will never return `Poll::Pending`, so we can use `sync_executor` to run those async functions like blocking functions.

## Running Tests

Integration tests rely on a local copy of nginx running. There's a docker-compose file you can use to easily set this up:

```sh
cd ./test-support
docker-compose up
cargo test
cargo test --no-default-features -F blocking -- blocking
```
