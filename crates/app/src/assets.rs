//! App assets: the `gpui-kit` component icons plus a few Lucide icons the
//! default set lacks (`assets/icons`, ISC licence next to them).

use std::borrow::Cow;

use gpui_kit::{AssetSource, SharedString};

/// Extra icons by name; `Icon::path` takes `icons/<name>.svg`.
const EXTRA: &[(&str, &[u8])] = &[
    (
        "icons/archive.svg",
        include_bytes!("../assets/icons/archive.svg"),
    ),
    (
        "icons/arrow-left.svg",
        include_bytes!("../assets/icons/arrow-left.svg"),
    ),
    (
        "icons/file-pen-line.svg",
        include_bytes!("../assets/icons/file-pen-line.svg"),
    ),
    (
        "icons/folder-input.svg",
        include_bytes!("../assets/icons/folder-input.svg"),
    ),
    ("icons/mail.svg", include_bytes!("../assets/icons/mail.svg")),
    ("icons/send.svg", include_bytes!("../assets/icons/send.svg")),
    ("icons/tag.svg", include_bytes!("../assets/icons/tag.svg")),
    (
        "icons/trash.svg",
        include_bytes!("../assets/icons/trash.svg"),
    ),
    ("icons/x.svg", include_bytes!("../assets/icons/x.svg")),
];

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<Cow<'static, [u8]>>> {
        match EXTRA.iter().find(|(p, _)| *p == path) {
            Some((_, bytes)) => Ok(Some(Cow::Borrowed(bytes))),
            None => gpui_kit::assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> gpui_kit::Result<Vec<SharedString>> {
        let mut all = gpui_kit::assets::Assets.list(path)?;
        all.extend(
            EXTRA
                .iter()
                .filter(|(p, _)| p.starts_with(path))
                .map(|(p, _)| SharedString::from(*p)),
        );
        all.sort();
        all.dedup();
        Ok(all)
    }
}
