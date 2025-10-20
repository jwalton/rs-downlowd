# downlowd

Downloading a file is easy. Just make an HTTP request, and write the results to a file, right? That works, but it doesn't cover a lot of corner cases. `downlowd` supports:

- Streaming the file to disk, instead of downloading it to memory and then writing it to disk. This is both faster and far more memory efficient for large files.
- Progress callback for displaying progress bar.
- Resuming file downloads.
- Files are written to disk as "filename.part" and then renamed to "filename" on completion, to make it obvious the file isn't complete.
- Automatic retries for flakey network connections and servers, with exponential backoff.
- Uses `content-disposition` header to retrieve the name of the file.
- Support for bandwidth restrictions.
- [Blocking client](./blocking) which is based on [ureq](https://docs.rs/ureq/latest/ureq/) so the blocking case doesn't depend on reqwest or tokio, making a smaller executable and faster build times.

## Documentation

See [the documentation at docs.rs](https://docs.rs/downlowd/latest/downlowd/).

## Crate Features

- **async** - Enabled by default. Provides [`Client`] for downloading files. This uses [reqwest](https://docs.rs/reqwest/latest/reqwest/) and [tokio](https://tokio.rs/) under the hood.
- **blocking** - Provides the [`blocking::Client`] for downloading files, which is based on [ureq](https://docs.rs/ureq/latest/ureq/). To use the blocking client, install will `cargo add downlowd --no-default-features -F blocking`.

## Usage

This is the simplest example:

```rust
# tokio_test::block_on(async {
use downlowd::Client;
# use temp_dir::TempDir;
# let dir = TempDir::new()?;
# let dirname = dir.path();

let client = Client::new();
let result = client
    .get("http://localhost:8089/hello.txt")
    .destination(dirname)
    .send()
    .await?;

assert_eq!(&result.path, &dirname.join("hello.txt"));
let file_contents = tokio::fs::read_to_string(&result.path).await?;
assert_eq!(file_contents, "hello world");
# Ok::<(), Box<dyn std::error::Error>>(())
# }).unwrap()
```

This is a short example, but it has a lot packed into it. First, since we've passed in a directory as the `destination`, this will work out what filename the file should be saved as In this case, the filename is derived from the URL, but if the server responds to a HEAD request with a `Content-Disposition` header with a filename, we'll use that filename. Note here we could also specify a filename instead of a directory name, and then `downlowd` would write our file to the specified filename.

If the file already exists, and has the correct length, then `downlowd` will just report success right away! If not, then we'll start downloading the file into a file named `hello.txt.part`. The file will be renamed to `hello.txt` once the download is complete. While the download is in progress, a "sidecar" file named `hello.txt.downloadinfo` will be written alongside the file which will contain cache information about the file (the etag header, the last-modified header, etc...). The sidecar file is used to help determine whether or not a file has changed on the server if the download is interrupted and needs to be resumed.

If there's an error during the download, such as a network error, or the transfer is interrupted, or the server returns a 5xx error, then `downlowd` will automatically retry the file, with an exponential backoff between retries. By default, `downlowd` will retry forever. Calling `max_retries()` will set a maximum number of tries, but note that `downlowd` will reset the retry counter if any progress is made downloading the file.

### Reporting Progress

There are a couple of ways you can hook into downlowd to report on progress. The `on_progress` handler is called once at the start of the download, and then whenever bytes are downloaded.

```rust
# tokio_test::block_on(async {
# use downlowd::Client;
# use temp_dir::TempDir;
# let dir = TempDir::new()?;
# let dirname = dir.path();

let client = Client::new();
let result = client
    .get("http://localhost:8089/hello.txt")
    .destination(dirname.join("file.txt"))
    .on_progress(|progress| {
        println!(
            "Downloaded {} of {} bytes",
            progress.bytes(),
            progress.remote_length().unwrap()
        );
    })
    .send()
    .await?;

assert_eq!(&result.path, &dirname.join("file.txt"));
# Ok::<(), Box<dyn std::error::Error>>(())
# }).unwrap();
```

The progress handle can also be used to cancel a download via `progress.cancel()`. There's quite a bit of data about the download that can be retrieved from the progress handle. You can see an [example in the examples folder](./examples/dl.rs) which uses [indicatif](https://docs.rs/indicatif/latest/indicatif) to render a pretty download progress bar. If you've cloned the repo, you can run it with something like:

```sh
cargo run https://releases.ubuntu.com/24.04.3/ubuntu-24.04.3-desktop-amd64.iso .
```

### Customizing Retries

You can also use the `on_retry()` method to register a handler that will be run immediately prior to a retry. This can be used to customize the backoff, or cancel the download:

```rust
# tokio_test::block_on(async {
# use std::time::Duration;
# use temp_dir::TempDir;
use downlowd::{Client, Error};
# let dir = TempDir::new()?;
# let dirname = dir.path();

let client = Client::new();
let result = client
    .get("http://localhost:8089/hello.txt")
    .destination(dirname)
    .on_retry(|r| {
        if matches!(r.error(), Error::FileChanged { .. }) {
            // No delay if the file changed.
            r.set_delay(Duration::ZERO);
        } else {
            r.set_delay(downlowd::exponential_backoff(
                Duration::from_secs(1),
                Duration::from_secs(30),
                r.retries(),
            ));
        }
    })
    .send()
    .await?;

# Ok::<(), Box<dyn std::error::Error>>(())
# }).unwrap();
```

Again, you can call `r.cancel()` here to not retry at all, and instead fail the entire download.

### Client Options

You can create a custom client using the `ClientBuilder`:

```rust
# tokio_test::block_on(async {
use downlowd::{ClientBuilder, Client};
# use temp_dir::TempDir;
# let dir = TempDir::new()?;
# let dirname = dir.path();

let client = ClientBuilder::new()
    .user_agent("my-cool-app")
    .header("Authorization", "Bearer secret-token")
    .build()?;

let result = client
    .get("http://localhost:8089/hello.txt")
    // Can set headers at the request level, too.
    .header("x-my-custom-header", "canon")
    .destination(dirname.join("file.txt"))
    .send()
    .await?;
# Ok::<(), Box<dyn std::error::Error>>(())
# }).unwrap()
```
