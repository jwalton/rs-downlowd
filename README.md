# file-downloader

There are many file downloaders out there for Rust, but this one is hands down the best.

Downloading a file is easy. Just make an HTTP request, and write the results to a file, right? There's actually a lot more to consider. best_file_downloader supports:

- Streaming the file to disk, instead of downloading it to memory and then writing it to disk. Better for large files.
- Progress callback for displaying progress bar.
- If part of the file is already on disk, download can be resumed. This is done by checking to see if the HEAD response for the file returns an `accept-ranges: bytes` header, and if so, adding a `range` header to the request.
- The ctime of the file is set to the `last-modified` time returned by the server. If the `last-modified` time doesn't match the ctime of an existing file, we know the file has changed on the server, and the download can't be resumed.
- Files are written to disk as "filename.part" and then renamed to "filename" on completion, to make it obvious the file isn't complete.
- Automatic retries for flakey network connections and servers.
- Uses `content-disposition` header to retrieve the name of the file.
- Support for bandwidth restrictions.

## Running Tests

Integration tests rely on a local copy of nginx running. There's a docker-compose file you can use to easily set this up:

```sh
cd ./test-support
docker-compose up
```
