use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gedcomkit::Node;
use ratatui::layout::Size;
use ratatui_image::Resize;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::Protocol;

use crate::person::People;

pub const WIDTH: u16 = 16;
pub const HEIGHT: u16 = 8;
const IMAGE_EXTENSIONS: [&str; 6] = ["jpg", "jpeg", "png", "gif", "webp", "bmp"];

/// Terminal images for the centre person, loaded once per file.
pub struct Portraits {
    picker: Option<Picker>,
    cache: HashMap<PathBuf, Option<Protocol>>,
}

impl Portraits {
    /// Asks the terminal which image protocol it supports; call after entering the alternate screen.
    pub fn detect() -> Self {
        Self {
            picker: Picker::from_query_stdio().ok(),
            cache: HashMap::new(),
        }
    }

    /// Whether the terminal shows real images rather than coloured half-blocks.
    pub fn has_graphics(&self) -> bool {
        self.picker
            .as_ref()
            .is_some_and(|p| p.protocol_type() != ProtocolType::Halfblocks)
    }

    pub fn protocol_name(&self) -> String {
        self.picker
            .as_ref()
            .map_or_else(|| "none".to_owned(), |p| format!("{:?}", p.protocol_type()))
    }

    /// The image at `path`, sized to fit the portrait area, or None when it cannot be shown.
    pub fn get(&mut self, path: &Path) -> Option<&Protocol> {
        let picker = self.picker.as_ref()?;
        self.cache
            .entry(path.to_path_buf())
            .or_insert_with(|| load(picker, path))
            .as_ref()
    }
}

fn load(picker: &Picker, path: &Path) -> Option<Protocol> {
    let image = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    picker
        .new_protocol(image, Size::new(WIDTH, HEIGHT), Resize::Fit(None))
        .ok()
}

/// The person's primary photo (Geni's `_PRIM Y`), else their first image, as a file next to the `.ged`.
pub fn portrait_path(people: &People, ged: &Path, xref: &str) -> Option<PathBuf> {
    let record = people.record(xref)?;
    let links: Vec<&Node> = record.all("OBJE").collect();
    let primary = links
        .iter()
        .find(|l| l.value_of("_PRIM").as_deref() == Some("Y"));
    let dir = ged.parent().unwrap_or(Path::new("."));
    primary
        .into_iter()
        .chain(links.iter())
        .filter_map(|link| image_file(people, link))
        .map(|file| dir.join(file))
        .find(|path| path.exists())
}

fn image_file(people: &People, link: &Node) -> Option<String> {
    let media = link
        .pointer()
        .and_then(|p| people.record(p))
        .unwrap_or(link);
    media.all("FILE").map(Node::logical_value).find(|file| {
        let lower = file.to_ascii_lowercase();
        !lower.starts_with("http")
            && IMAGE_EXTENSIONS
                .iter()
                .any(|ext| lower.ends_with(&format!(".{ext}")))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_photo_wins_and_missing_files_are_skipped() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("media")).unwrap();
        std::fs::write(dir.path().join("media/second.jpg"), b"x").unwrap();
        std::fs::write(dir.path().join("media/primary.png"), b"x").unwrap();
        let ged = dir.path().join("family.ged");
        let doc = gedcomkit::Document::parse(
            "0 HEAD\n0 @I1@ INDI\n1 OBJE @O1@\n1 OBJE @O2@\n1 OBJE @O3@\n2 _PRIM Y\n\
             0 @O1@ OBJE\n1 FILE media/missing.jpg\n0 @O2@ OBJE\n1 FILE media/second.jpg\n\
             0 @O3@ OBJE\n1 FILE media/primary.png\n0 TRLR\n",
        )
        .unwrap();
        let people = People::new(&doc);
        assert_eq!(
            portrait_path(&people, &ged, "@I1@"),
            Some(dir.path().join("media/primary.png"))
        );
    }

    #[test]
    #[ignore]
    fn preview_portrait_lookup() {
        let path = PathBuf::from(std::env::var("GEDCLI_PREVIEW").unwrap());
        let store = crate::store::Store::open(path.clone()).unwrap();
        let people = People::new(&store.doc);
        let xref = std::env::var("GEDCLI_PREVIEW_ID").unwrap();
        let found = portrait_path(&people, &path, &xref);
        println!("portrait: {found:?}");
        let picker = Picker::halfblocks();
        println!(
            "decoded: {}",
            found.is_some_and(|p| load(&picker, &p).is_some())
        );
    }
}
