//! Kinds of media a model reads or produces: text, images, PDFs, audio
//! and video.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// One kind of media, as models.dev names it in a model's `modalities`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modality {
    /// Plain text.
    Text,
    /// Images.
    Image,
    /// PDF documents.
    Pdf,
    /// Audio.
    Audio,
    /// Video.
    Video,
}

impl Modality {
    /// Every kind, in display order.
    pub const ALL: [Self; 5] = [Self::Text, Self::Image, Self::Pdf, Self::Audio, Self::Video];

    /// Name, such as `"image"`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::Pdf => "pdf",
            Self::Audio => "audio",
            Self::Video => "video",
        }
    }

    /// The media kind a MIME type names: `image/*`, `audio/*`,
    /// `video/*` or `application/pdf`. `None` for anything else.
    #[must_use]
    pub fn of_mime(mime: &str) -> Option<Self> {
        let mime = mime.trim().to_ascii_lowercase();
        let (top, sub) = mime.split_once('/')?;
        match top {
            "image" => Some(Self::Image),
            "audio" => Some(Self::Audio),
            "video" => Some(Self::Video),
            "application" if sub == "pdf" => Some(Self::Pdf),
            _ => None,
        }
    }

    /// Parse a name produced by [`Self::as_str`].
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|i| i.as_str() == s)
    }
}

/// A set of [`Modality`] kinds. Empty means the set is unknown.
/// Serializes as a list of names.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Modalities(u8);

impl Modalities {
    /// The set holding `values`.
    #[must_use]
    pub const fn of(values: &[Modality]) -> Self {
        let mut bits = 0;
        let mut i = 0;
        while i < values.len() {
            bits |= 1 << values[i] as u8;
            i += 1;
        }
        Self(bits)
    }

    /// Whether `kind` is in the set.
    #[must_use]
    pub fn contains(self, kind: Modality) -> bool {
        self.0 & (1 << kind as u8) != 0
    }

    /// Whether the set is empty, meaning unknown.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// Whether the set is known and leaves out `kind`.
    #[must_use]
    pub fn lacks(self, kind: Modality) -> bool {
        !self.is_empty() && !self.contains(kind)
    }

    /// The kinds in the set, in display order.
    pub fn iter(self) -> impl Iterator<Item = Modality> {
        Modality::ALL.into_iter().filter(move |i| self.contains(*i))
    }

    /// Names joined by spaces, such as `"text image pdf"`.
    #[must_use]
    pub fn label(self) -> String {
        self.iter()
            .map(Modality::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl FromIterator<Modality> for Modalities {
    fn from_iter<I: IntoIterator<Item = Modality>>(iter: I) -> Self {
        Self(iter.into_iter().fold(0, |bits, i| bits | 1 << i as u8))
    }
}

impl Serialize for Modalities {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter())
    }
}

impl<'de> Deserialize<'de> for Modalities {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Vec::<Modality>::deserialize(deserializer)?
            .into_iter()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mime_type_names_its_kind() {
        assert_eq!(Modality::of_mime("image/png"), Some(Modality::Image));
        assert_eq!(Modality::of_mime(" Audio/MPEG"), Some(Modality::Audio));
        assert_eq!(Modality::of_mime("video/mp4"), Some(Modality::Video));
        assert_eq!(Modality::of_mime("application/pdf"), Some(Modality::Pdf));
        assert_eq!(Modality::of_mime("application/json"), None);
        assert_eq!(Modality::of_mime("text/plain"), None);
        assert_eq!(Modality::of_mime(""), None);
    }

    #[test]
    fn empty_inputs_lack_nothing() {
        assert!(!Modalities::default().lacks(Modality::Image));
        let text = Modalities::of(&[Modality::Text]);
        assert!(text.lacks(Modality::Image));
        assert!(!text.lacks(Modality::Text));
    }

    #[test]
    fn inputs_roundtrip_and_label_in_order() {
        let set: Modalities = [Modality::Pdf, Modality::Text, Modality::Image]
            .into_iter()
            .collect();
        assert_eq!(set.label(), "text image pdf");
        let v = serde_json::to_value(set).unwrap();
        assert_eq!(v, serde_json::json!(["text", "image", "pdf"]));
        assert_eq!(serde_json::from_value::<Modalities>(v).unwrap(), set);
    }
}
