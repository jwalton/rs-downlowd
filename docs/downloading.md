# Downloading Files

Downloading a file is easy. Just make an HTTP request, and write the results to a file, right? This talks about some of the cases that need to be handled, and how `downlowd` handles them.

## Working out a file's filename

When we download a file, `downlowd` allows you to specify either a file to write to, or a folder to store the downloaded file in. If you specify a folder, how do we know what filename to use? The naive approach is to take the last segment of the URL (and, if that's all that is available, this is what `downlowd` falls back to). But, the server can optionally supply a [Content-Disposition](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Disposition) header, which can specify a filename. This means you might download a file from a url like `https://example.com/files/0001`, but end up with a file on disk named `awesomeface.jpg`.

## Resuming a Download

If you're downloading a file, and the transfer is interrupted, it's nice to be able to resume the download from where you left off. In order to do this, you need to know that file hasn't changed on the server, and that the server supports downloading part of a file.

There are three HTTP headers that can be used to track if a file has changed; [Last-Modified](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Last-Modified), [ETag](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/ETag), and `Content-Length`. If these change, then we know the file has changed.  (Note that an `ETag` header that starts with "W/" is considered a "weak validator", and shouldn't be used to validate a file hasn't changed.)

Deciding whether or not a server supports transferring part of a file can be accomplished through the [Accept-Ranges header](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Accept-Ranges) and specifically we're looking for `Accept-Ranges: bytes` (see [RFC9110](https://www.rfc-editor.org/info/rfc9110/#field.accept-ranges)).

Assuming we know the file hasn't changed, and we know the server supports sending byte ranges, we can send a [Range header](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Range) such as `Range: bytes 1000-` to download bytes 1000 through to the end of the file.

At this point, the naive approach would be to send a HEAD request to the server for the file, see what its last-modified date is, and verify that the accept-ranges header is present, and then we can proceed to try to download part of the file. That works, but we can be more efficient and do this in a single network request. There's one more header involved which is [If-Range](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/If-Range).

Basically the idea is, if we know we have the first 1000 bytes of the file, we'd send a request that has `Range: 1000-` and `If-Range: [last-modified-date]` or `If-Range: [etag]`, and then the server will do one of two things; if the file hasn't changed and the server supports ranges, it will reply with "206 - Partial Content" and a "Content-Range" header indicating what bytes it's sending back, or if the file has changed and/or the server doesn't support range headers, the server will reply with a regular "200 - OK" and send us the whole file from the start. Using `If-Range` is also a more reliable solution than trying to send a HEAD request, as many servers don't follow the HTTP specification very well, and may neglect to send the ETag or Last-Modified headers when presented with a HEAD request.

`downlowd` needs to keep track of the ETag, last modified date, and length of the file between retries (possibly between program executions), as well as data from the partially downloaded file. Assuming you're downloading `bigfile.tgz`, `downlowd` will write a `bigfile.tgz.part` file, containing the partially downloaded file, and a `bigfile.tgz.downloadinfo` sidecar file, containing any information we know about the file. If, while creating a download, you specify the [`last_modified`](https://docs.rs/downlowd/latest/downlowd/struct.Download.html#method.last_modified) or [`etag`](https://docs.rs/downlowd/latest/downlowd/struct.Download.html#method.etag) before starting the download, `downlowd` will trust the values you pass in and the sidecar file will be ignored.

## Rate Limiting a Download

Rate limiting a download is actually very easy; the trick is to just stop reading bytes from the network socket.  We're exploiting something here called "backpressure". As data arrives at your network card (or NIC), the NIC will write that data into a ring buffer that's shared between the OS and the NIC's driver, and then the kernel reads that data out and puts it into your socket's receive buffer. If your application stops reading data from the socket, the socket's receive buffer will fill up.  As this happens, the socket will start reducing the TCP "receive window" - the amount of data it can accept without an acknowledgement - until it eventually drops to zero. The sender will stop sending until the window opens up again.  If the sender keeps trying to send packets at this point, there will be nowhere for them to be written, and they will be dropped (and not ACKed).  Standard TCP flow control takes care of all of this for us.

In other words, if we want to download at a specific speed, all we need to do is `sleep()` for a short while if we're downloading bytes faster than this speed.

`downlowd` accomplishes this through a simple token-bucket style rate limiter. As we download data, we add tokens to the bucket. If the bucket is too full, we sleep and wait for tokens to drain.
