# Local printing and print PDFs

File > Print on the desktop prepares one sheet per visible top-level frame on the current
page. Choose every frame or the selected frames, paper, orientation, scale, grayscale,
copies and duplex. The preview shows the actual frame on white paper, including its placement
and grayscale conversion. Previous/Next changes the preview sheet. Save print PDF writes the
same annotation-free PDF through the ordinary save picker and does not contact a printer.

The initial paper layout is A4. The queue retains its own media default until a paper is
explicitly changed. Frame size also leaves local queue media selection to the queue. The
Printer list marks the default queue. The local job is submitted only by pressing Print;
Cancel print job waits for the originating queue's own job receipt and asks CUPS to cancel
that job. It never cancels a guessed or foreign job and never resubmits a pending job. A
cancellation request cannot recall pages already printed. Closing the dialog does not hide
subsequent handover failures. Application shutdown waits at most 25 seconds for outstanding
handover workers and records any that remain unfinished.

Network printer is optional (`network-print`, enabled by default on desktop). Enter a numeric
local printer address and press Query; there is no automatic discovery or listener. The app
shows the printer's capabilities, requires per-job confirmation and follows the submitted
job's status. An uncertain submission is not repeated automatically. IPP/WS-Print transports
are plain HTTP, restricted by the local-address policy; `ipps` and public addresses are refused.
Turn off the feature with `--no-default-features` to omit network-print code and backends.

The engine command `file.print` requests the desktop dialog. Programmatic control calls drop
that request and cannot submit a printer job. `file.printPdf` prepares a PDF without opening a
dialog or changing the design revision:

```json
{"command":"file.printPdf","params":{"paper":"A4","orientation":"auto","scale":"shrink","grayscale":true}}
```

Its optional parameters are `ids` (unique visible top-level current-page frame ids), `paper`
(`A3`, `A4`, `A5`, `Letter`, `Legal`, `Tabloid`, or `[width,height]` in points), `orientation`
(`auto`, `portrait`, `landscape`), `scale` (`shrink`, `fit`), `margin` (nonnegative points),
`grayscale` (boolean), and native `path`. Orientation, scale and margin require paper; the
margin cannot exceed one quarter of its short side. Without paper, sheets use frame sizes.
Without ids, selected eligible frames take precedence, otherwise all eligible frames are used.
Invalid, duplicate, hidden, child or other-page ids are refused. At most 10,000 sheets are
allowed. Without path, the result contains base64 `data`; with path, existing native file
access rules apply. Both results include the PDF byte count and sheet placement metadata.

The browser has no native printer queue or sockets. Pathless `file.printPdf` remains an offline
PDF preparation operation; its caller can open/download the returned PDF and use the browser's
PDF viewer to print. The host broker must reject a native `path` rather than attempting a server
filesystem write.

This local adaptation qualifies macOS ARM64 plus the existing WASM build. The pinned portable
sources and licenses are in `vendor/`; it does not claim a Windows spooler build. Real printer
devices, remote printer availability, release signing and notarization need their own runtime
validation. Automated print tests use synthetic/in-memory or loopback printers and injected
queue runners, and never send a real printer job.
