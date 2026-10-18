//! Parallel directory scanner and the compact arena tree it produces.

use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::Arc;

pub const NONE: u32 = u32::MAX;

/// File categories used for colouring the map and the legend.
pub const CATS: [(&str, [u8; 3]); 8] = [
    ("Images", [232, 100, 146]),
    ("Video", [139, 108, 240]),
    ("Audio", [47, 191, 168]),
    ("Archives", [232, 163, 61]),
    ("Code", [74, 144, 226]),
    ("Documents", [95, 191, 106]),
    ("Apps", [229, 105, 79]),
    ("Other", [124, 138, 160]),
];

pub fn category(name: &str) -> u8 {
    let ext = match name.rsplit_once('.') {
        Some((stem, e)) if !stem.is_empty() => e.to_ascii_lowercase(),
        _ => return 7,
    };
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "heif" | "bmp" | "tif" | "tiff" | "svg" | "raw"
        | "cr2" | "nef" | "arw" | "dng" | "psd" | "ico" | "avif" => 0,
        "mp4" | "mov" | "mkv" | "avi" | "webm" | "m4v" | "wmv" | "flv" | "mpg" | "mpeg" => 1,
        "mp3" | "wav" | "flac" | "aac" | "m4a" | "ogg" | "opus" | "aiff" | "wma" | "mid" => 2,
        "zip" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "tar" | "zst" | "dmg" | "iso" | "pkg" | "img"
        | "vmdk" | "qcow2" | "vhd" | "vhdx" => 3,
        "rs" | "c" | "h" | "cpp" | "hpp" | "cc" | "cs" | "java" | "kt" | "swift" | "go" | "py" | "js" | "jsx"
        | "ts" | "tsx" | "rb" | "php" | "html" | "css" | "scss" | "json" | "toml" | "yaml" | "yml" | "sh"
        | "sql" | "lua" | "md" | "xml" | "o" | "a" | "rlib" | "rmeta" | "d" | "map" => 4,
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "rtf" | "odt" | "ods" | "pages"
        | "numbers" | "key" | "epub" | "csv" | "log" => 5,
        "exe" | "dll" | "so" | "dylib" | "app" | "msi" | "bin" | "sys" | "lib" | "framework" | "wasm" | "jar"
        | "apk" | "deb" | "rpm" | "node" => 6,
        _ => 7,
    }
}

pub struct Node {
    pub name: String,
    pub size: u64,
    pub files: u64,
    pub parent: u32,
    pub children: Vec<u32>,
    pub is_dir: bool,
    pub cat: u8,
}

pub struct Tree {
    pub nodes: Vec<Node>,
    pub root_path: PathBuf,
    pub cat_bytes: [u64; 8],
    pub cat_files: [u64; 8],
}

impl Tree {
    pub fn path_of(&self, mut id: u32) -> PathBuf {
        let mut parts = Vec::new();
        while self.nodes[id as usize].parent != NONE {
            parts.push(self.nodes[id as usize].name.as_str());
            id = self.nodes[id as usize].parent;
        }
        let mut p = self.root_path.clone();
        for part in parts.iter().rev() {
            p.push(part);
        }
        p
    }
    pub fn depth_of(&self, mut id: u32) -> usize {
        let mut d = 0;
        while self.nodes[id as usize].parent != NONE {
            id = self.nodes[id as usize].parent;
            d += 1;
        }
        d
    }
}

#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub bytes: AtomicU64,
    pub cancel: AtomicBool,
}

struct Entry {
    name: String,
    size: u64,
    files: u64,
    is_dir: bool,
    children: Vec<Entry>,
}

#[cfg(unix)]
fn disk_size(md: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    md.blocks() * 512
}
#[cfg(not(unix))]
fn disk_size(md: &fs::Metadata) -> u64 {
    md.len()
}

/// Virtual or duplicated file systems that would only add noise or double counting.
fn skipped(p: &Path) -> bool {
    #[cfg(target_os = "macos")]
    const SKIP: &[&str] = &["/System/Volumes", "/dev", "/Volumes", "/private/var/vm", "/.vol", "/.Spotlight-V100", "/.fseventsd"];
    #[cfg(target_os = "linux")]
    const SKIP: &[&str] = &["/proc", "/sys", "/dev", "/run", "/mnt", "/media"];
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    const SKIP: &[&str] = &[];
    SKIP.iter().any(|s| p == Path::new(s))
}

fn scan_dir(path: &Path, prog: &Progress) -> Vec<Entry> {
    let Ok(rd) = fs::read_dir(path) else { return Vec::new() };
    prog.dirs.fetch_add(1, Relaxed);
    let items: Vec<fs::DirEntry> = rd.filter_map(Result::ok).collect();
    items
        .into_par_iter()
        .filter_map(|e| {
            if prog.cancel.load(Relaxed) {
                return None;
            }
            let ft = e.file_type().ok()?;
            if ft.is_symlink() {
                return None;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            if ft.is_dir() {
                let p = e.path();
                if skipped(&p) {
                    return None;
                }
                let children = scan_dir(&p, prog);
                let size = children.iter().map(|c| c.size).sum();
                let files = children.iter().map(|c| c.files).sum();
                Some(Entry { name, size, files, is_dir: true, children })
            } else {
                let size = disk_size(&e.metadata().ok()?);
                prog.files.fetch_add(1, Relaxed);
                prog.bytes.fetch_add(size, Relaxed);
                Some(Entry { name, size, files: 1, is_dir: false, children: Vec::new() })
            }
        })
        .collect()
}

fn flatten(t: &mut Tree, mut e: Entry, parent: u32) -> u32 {
    let id = t.nodes.len() as u32;
    let cat = if e.is_dir { 7 } else { category(&e.name) };
    if !e.is_dir {
        t.cat_bytes[cat as usize] += e.size;
        t.cat_files[cat as usize] += 1;
    }
    t.nodes.push(Node { name: std::mem::take(&mut e.name), size: e.size, files: e.files, parent, children: Vec::new(), is_dir: e.is_dir, cat });
    e.children.sort_unstable_by(|a, b| b.size.cmp(&a.size));
    let kids: Vec<u32> = e.children.into_iter().map(|c| flatten(t, c, id)).collect();
    t.nodes[id as usize].children = kids;
    id
}

pub fn scan(root: &Path, prog: &Arc<Progress>) -> Option<Tree> {
    let pool = rayon::ThreadPoolBuilder::new().stack_size(32 << 20).build().ok()?;
    let children = pool.install(|| scan_dir(root, prog));
    if prog.cancel.load(Relaxed) {
        return None;
    }
    let size = children.iter().map(|c| c.size).sum();
    let files = children.iter().map(|c| c.files).sum();
    let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| root.display().to_string());
    let mut t = Tree { nodes: Vec::new(), root_path: root.to_path_buf(), cat_bytes: [0; 8], cat_files: [0; 8] };
    flatten(&mut t, Entry { name, size, files, is_dir: true, children }, NONE);
    Some(t)
}

pub fn fmt_bytes(b: u64) -> String {
    let base: f64 = if cfg!(target_os = "macos") { 1000.0 } else { 1024.0 };
    let units = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= base && i < units.len() - 1 {
        v /= base;
        i += 1;
    }
    match i {
        0 => format!("{b} B"),
        _ if v >= 100.0 => format!("{v:.0} {}", units[i]),
        _ if v >= 10.0 => format!("{v:.1} {}", units[i]),
        _ => format!("{v:.2} {}", units[i]),
    }
}
