use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use gedcomkit::Node;
use ratatui::layout::Size;
use ratatui_image::Resize;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::Protocol;

use crate::person::People;

/// Image area in character cells: the large one for the centre person, the small one on other cards.
pub const PORTRAIT: (u16, u16) = (16, 8);
pub const THUMB: (u16, u16) = (6, 3);
const IMAGE_EXTENSIONS: [&str; 6] = ["jpg", "jpeg", "png", "gif", "webp", "bmp"];

type Key = (PathBuf, (u16, u16));

/// Terminal images decoded on background threads, each kept once per file and size.
pub struct Portraits {
    picker: Option<Picker>,
    cache: HashMap<Key, Option<Protocol>>,
    requests: Option<Sender<Key>>,
    results: Receiver<(Key, Option<Protocol>)>,
}

impl Portraits {
    /// Asks the terminal which image protocol it supports and starts the decoding threads; call after entering the alternate screen.
    pub fn detect() -> Self {
        let picker = Picker::from_query_stdio().ok();
        let (done, results) = mpsc::channel();
        let requests = picker.clone().map(|picker| start_workers(picker, done));
        Self {
            picker,
            cache: HashMap::new(),
            requests,
            results,
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

    /// Queues the image at `path` for an area of `size` cells; returns false once it is known not to load.
    pub fn request(&mut self, path: &Path, size: (u16, u16)) -> bool {
        let Some(requests) = &self.requests else {
            return false;
        };
        let key = (path.to_path_buf(), size);
        if let Some(loaded) = self.cache.get(&key) {
            return loaded.is_some();
        }
        self.cache.insert(key.clone(), None);
        requests.send(key).is_ok()
    }

    /// Stores the images decoded since the last call; returns whether any arrived.
    pub fn receive(&mut self) -> bool {
        let mut arrived = false;
        for (key, protocol) in self.results.try_iter() {
            self.cache.insert(key, protocol);
            arrived = true;
        }
        arrived
    }

    pub fn cached(&self, path: &Path, size: (u16, u16)) -> Option<&Protocol> {
        self.cache.get(&(path.to_path_buf(), size))?.as_ref()
    }
}

fn start_workers(picker: Picker, done: Sender<(Key, Option<Protocol>)>) -> Sender<Key> {
    let (requests, queue) = mpsc::channel::<Key>();
    let queue = Arc::new(Mutex::new(queue));
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get().min(4));
    for _ in 0..threads {
        let (picker, done, queue) = (picker.clone(), done.clone(), Arc::clone(&queue));
        std::thread::spawn(move || {
            while let Ok(key) = queue
                .lock()
                .map_err(drop)
                .and_then(|q| q.recv().map_err(drop))
            {
                let protocol = load(&picker, &key.0, key.1);
                if done.send((key, protocol)).is_err() {
                    break;
                }
            }
        });
    }
    requests
}

fn load(picker: &Picker, path: &Path, (width, height): (u16, u16)) -> Option<Protocol> {
    let image = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    picker
        .new_protocol(image, Size::new(width, height), Resize::Fit(None))
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
            found.is_some_and(|p| load(&picker, &p, PORTRAIT).is_some())
        );
    }
}
