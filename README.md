# file-downloader

Downloading a file is easy. Just make an HTTP request, and write the results to a file, right? There's actually a lot more to consider. This library supports:

- Streaming the file to disk, instead of downloading it to memory and then writing it to disk. Better for large files.
- Progress callback for displaying progress bar.
- Resuming file downloads.
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
