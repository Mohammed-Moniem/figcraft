# Local macOS print stack

The print/job/WSD sources are pinned to craft-libs commit `1cece76ac446315f81694151a6626588faa772c4` under MIT OR Apache-2.0; original licenses and copyright notices remain. This narrow workspace carries CUPS and the explicit-address IPP/WS-Print registry. It adds no automatic network discovery or listener. The source pin and file identities are recorded in `../native-print-pins.json`.

This local product qualifies macOS ARM64 and the app's WASM build. Its unclaimed Windows spooler dependencies and RGB/GDI facade are omitted. Upstream Windows implementation files remain for provenance but are not a Windows build claim. Current coupled craft-ui is preserved; separate pinned printing/capability controls live in `../craft-print-ui`.

Local adaptations: narrow portable workspace paths, omitted Windows/GDI dependency surface, and CUPS submission locale set consistently with lpstat so a submitted job's own receipt can be parsed for explicit cancellation. The app's cancellation helper validates the exact originating queue/id, never retries a submission, and reports uncertain cancellation honestly.
