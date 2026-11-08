# Icons

Lucide SVGs vendored for the client, one file per icon the app draws.
Source: the Lucide catalog as packaged by the previous desktop stack's
asset crate (0.7.0), pinned by the E00 spike's Cargo.lock. License:
ISC (see LICENSE-LUCIDE); a portion of the catalog derives from the
Feather project, noted in that file. Icons are loaded through
egui_extras' SVG loader at runtime; nothing fetches from the network.
