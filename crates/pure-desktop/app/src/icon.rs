//! The window icon, decoded from the embedded brand art.
//!
//! eframe applies `ViewportBuilder::with_icon` on X11; Windows, macOS
//! and Wayland take the icon from the packaged resources (the `.ico`
//! embed and the hicolor set) instead. This module is therefore best
//! effort: a failed decode costs the icon, never the launch.

/// The window icon for `ViewportBuilder::with_icon`: the embedded app
/// icon at 256 px, or `None` when the art fails to decode. Applies on
/// X11 only.
#[must_use]
pub fn window_icon() -> Option<egui::IconData> {
    let decoded = image::load_from_memory(crate::assets::ICON_PNG)
        .ok()?
        .to_rgba8();
    Some(egui::IconData {
        width: decoded.width(),
        height: decoded.height(),
        rgba: decoded.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_brand_art_decodes_non_empty() {
        let icon = super::window_icon().expect("the embedded icon decodes");
        assert_eq!(icon.width, 256);
        assert_eq!(icon.height, 256);
        assert!(!icon.rgba.is_empty());
    }
}
