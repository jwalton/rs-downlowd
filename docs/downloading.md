# Downloading Files

Downloading a file is easy. Just make an HTTP request, and write the results to a file, right? This talks about some of the cases that need to be handled, and how best_file_downloader handles them.

## Working out a file's filename

When we download a file, best_file_downloader allows you to specify either a file to write to, or a folder to store the downloaded file in. If you specify a folder, how do we know what filename to use? The naive approach is to take the last segment of the URL (and, if that's all that is available, this is what best_file_downloader falls back to). But, the server can optionally supply a [Content-Disposition](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Disposition) header, which can specify a filename. This means you might download a file from a url like `https://example.com/files/0001`, but end up with a file on disk named `awesomeface.jpg`.

## Resuming a Download

If you're downloading a file, and the transfer is interrupted, it's nice to be able to resume the download from where you left off. In order to do this, you need to know that file hasn't changed on the server, and that the server supports downloading part of a file.

There are three HTTP headers than can be used to track if a file has changed; [Last-Modified](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Last-Modified), [Etag](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/ETag), and `Content-Length`. If any of these change, then we know the file has changed. best_file_downloader needs to keep track of these values between retries (possible between program executions), as well as data from the partially downloaded file. Assuming you're downloading `bigfile.tgz`, best_file_downloader will write a `bigfile.tgz.part` file, containing the partially downloaded file, and a `bigfile.tgz.downloadinfo` sidecar file, containing any information we know about the file. If you call `last_modified()` or `etag()` before starting the download, best_file_downloader will trust the values you pass in instead of using the sidecar file.

As to deciding whether or not a server supports transfering only part of a file, the header we're looking for is called [Accept-Ranges](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Accept-Ranges) and specifically we're looking for `Accept-Ranges: bytes` (there aren't really any other values this header can take on, as "bytes" is the only unit defined by RFC 7233).

Assuming we know the file hasn't changed, and we know the server supports sending ranges, we can send a [Range header](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Range) like `Range: 1000-` to download bytes 1000 through to the end of the file.

So at this point, the naive approach would be to send a HEAD request to the server for the file, see what it's last-modified date is, and verify that the accept-ranges header is present, and then we can proceed to try to download part of the file. That works, but we can be more efficient and do this in a single network request. There's one more header involved which is [If-Range](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/If-Range).

Basically the idea is, if we know we have the first 1000 bytes of the file, we'd send a request that has `Range: 1000-` and `If-Range: [last-modified-date]` or `If-Range: [etag]`, and then the server will do one of two things; if the file hasn't changed and the server supports ranges, it will reply with "206 - Partial Content" and a "Content-Range" header indicating what bytes it's sending back, or if the file has change and/or the server doesn't support range headers, the server will reply with a regular "200 - OK" and send us the whole file from the start.  Using `If-Range` is also a more reliable solution than trying to send a HEAD request, as many servers don't follow the HTTP specification very well, and are likely to neglect to send the Etag or Last-Modified headers when presented with a HEAD request.
