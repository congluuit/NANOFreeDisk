//! Pictures are built in memory (shapes on a gradient, from a seed) and saved as
//! JPEG or PNG, then changed the way chat apps and editors change photos.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageFormat, RgbImage, imageops};
use nfd_core::exec::{self, ExecOptions, ItemStatus, QuarantineRoots};
use nfd_core::plan::DeleteMode;
use nfd_core::safety::{LiveProbe, Origin};
use nfd_core::scanner::{self, ScanOptions};

use super::cache::MemoryImageCache;
use super::fingerprint::Fingerprint;
use super::plan::{SimilarSelection, SimilarVerifier, plan};
use super::*;
use crate::hash::LiveSource;
use crate::plan::{Action, PlanError};

/// xorshift64, the same as the calibration script.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1))
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn f(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn byte(&mut self) -> u8 {
        (self.next() >> 56) as u8
    }
}

/// A "photo": twelve discs on a two-way gradient, laid out in fractions of the size.
fn picture(seed: u64, w: u32, h: u32) -> RgbImage {
    let mut r = Rng::new(seed);
    let discs: Vec<(f64, f64, f64, [u8; 3])> = (0..12)
        .map(|_| {
            (
                r.f(),
                r.f(),
                0.05 + r.f() * 0.2,
                [r.byte(), r.byte(), r.byte()],
            )
        })
        .collect();
    let a = r.f();
    let scale = f64::from(w.min(h));
    RgbImage::from_fn(w, h, |x, y| {
        let (fx, fy) = (f64::from(x) / f64::from(w), f64::from(y) / f64::from(h));
        for (cx, cy, rad, col) in discs.iter().rev() {
            let (dx, dy) = (
                f64::from(x) - cx * f64::from(w),
                f64::from(y) - cy * f64::from(h),
            );
            if dx * dx + dy * dy <= (rad * scale) * (rad * scale) {
                return image::Rgb(*col);
            }
        }
        let t = a * fx + (1.0 - a) * fy;
        image::Rgb([
            (30.0 + 180.0 * t) as u8,
            (200.0 - 150.0 * t) as u8,
            (60.0 + 120.0 * fx) as u8,
        ])
    })
}

fn jpeg(img: &RgbImage, quality: u8) -> Vec<u8> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality)
        .encode_image(img)
        .expect("jpeg");
    out
}

fn png(img: &RgbImage) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(img.clone())
        .write_to(&mut out, ImageFormat::Png)
        .expect("png");
    out.into_inner()
}

fn resize(img: &RgbImage, w: u32, h: u32) -> RgbImage {
    imageops::resize(img, w, h, imageops::FilterType::Triangle)
}

fn brighter(img: &RgbImage, by: u8) -> RgbImage {
    let mut out = img.clone();
    for p in out.pixels_mut() {
        for c in &mut p.0 {
            *c = c.saturating_add(by);
        }
    }
    out
}

/// A JPEG with an EXIF orientation tag (APP1 right after the start marker).
fn with_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"II*\0");
    tiff.extend_from_slice(&8u32.to_le_bytes());
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend_from_slice(&0x0112u16.to_le_bytes());
    tiff.extend_from_slice(&3u16.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&orientation.to_le_bytes());
    tiff.extend_from_slice(&[0, 0]);
    tiff.extend_from_slice(&0u32.to_le_bytes());
    let mut app1 = b"Exif\0\0".to_vec();
    app1.extend_from_slice(&tiff);
    let len = u16::try_from(app1.len() + 2).expect("short");
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&app1);
    out.extend_from_slice(&jpeg[2..]);
    out
}

fn tree(root: &Path) -> ScanTree {
    scanner::scan(root, &ScanOptions::default(), None).expect("scan")
}

fn no_gate() -> SafetyGate {
    SafetyGate::new(&|_: &str| None)
}

fn opts() -> SimilarOptions {
    SimilarOptions {
        min_size: 1,
        ..SimilarOptions::default()
    }
}

fn hashed(root: &Path, cache: Option<&mut dyn ImageCache>) -> HashedSet {
    let t = tree(root);
    find_hashed(&[&t], &no_gate(), &opts(), cache, &LiveSource, &|_| {}).expect("find")
}

fn names(r: &SimilarReport, root: &Path) -> Vec<Vec<String>> {
    r.groups
        .iter()
        .map(|g| {
            let mut v: Vec<String> = g
                .photos
                .iter()
                .map(|p| {
                    p.path
                        .strip_prefix(root)
                        .expect("inside")
                        .to_string_lossy()
                        .replace('\\', "/")
                })
                .collect();
            v.sort();
            v
        })
        .collect()
}

fn report(set: &HashedSet, preset: Preset) -> SimilarReport {
    set.report(preset, None, &|_| {}).expect("report")
}

/// The original and what happens to it, in a temporary folder.
fn library(root: &Path) {
    let base = picture(1, 800, 600);
    let w = |name: &str, bytes: Vec<u8>| fs::write(root.join(name), bytes).expect("write");
    w("original.jpg", jpeg(&base, 92));
    w("original.png", png(&base));
    w("half.jpg", jpeg(&resize(&base, 400, 300), 92));
    // what a chat app sends: smaller and heavily compressed
    w("chat.jpg", jpeg(&resize(&base, 320, 240), 30));
    w("brighter.jpg", jpeg(&brighter(&base, 20), 92));
    let crop = imageops::crop_imm(&base, 24, 18, 752, 564).to_image();
    w("crop.jpg", jpeg(&crop, 90));
    // never the same picture
    w("mirrored.jpg", jpeg(&imageops::flip_horizontal(&base), 92));
    w("other.jpg", jpeg(&picture(101, 800, 600), 92));
    w("stretched.jpg", jpeg(&resize(&base, 1600, 600), 92));
}

#[test]
fn look_alikes_group_and_other_pictures_do_not() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path();
    library(r);
    let set = hashed(r, None);
    assert_eq!(set.photos.len(), 9, "{:?}", set.skipped_samples);

    let strict = report(&set, Preset::Strict);
    assert_eq!(strict.groups.len(), 1, "{:?}", names(&strict, r));
    let g = &names(&strict, r)[0];
    for alike in [
        "original.jpg",
        "original.png",
        "half.jpg",
        "chat.jpg",
        "brighter.jpg",
    ] {
        assert!(g.iter().any(|n| n == alike), "{alike} missing from {g:?}");
    }
    for never in ["mirrored.jpg", "other.jpg", "stretched.jpg"] {
        assert!(!g.iter().any(|n| n == never), "{never} grouped: {g:?}");
    }
    // the best one has the most pixels, then the largest file
    let best = &strict.groups[0].photos[0];
    assert_eq!((best.width, best.height), (800, 600));
    assert!(
        strict.groups[0]
            .photos
            .iter()
            .filter(|p| (p.width, p.height) == (800, 600))
            .all(|p| p.size <= best.size)
    );
    assert_eq!(best.distance, 0);
    assert!(
        strict.groups[0]
            .photos
            .iter()
            .all(|p| p.identical_to.is_none())
    );
    assert_eq!(
        strict.groups[0].extra,
        strict.groups[0].photos[1..]
            .iter()
            .map(|p| p.size)
            .sum::<u64>()
    );

    // a slight crop joins from Normal on; the others never do
    for preset in [Preset::Normal, Preset::Loose] {
        let rep = report(&set, preset);
        assert_eq!(rep.groups.len(), 1, "{preset:?}");
        let g = &names(&rep, r)[0];
        assert!(g.iter().any(|n| n == "crop.jpg"), "{preset:?}: {g:?}");
        for never in ["mirrored.jpg", "other.jpg", "stretched.jpg"] {
            assert!(
                !g.iter().any(|n| n == never),
                "{preset:?}: {never} in {g:?}"
            );
        }
    }
}

#[test]
fn the_exif_orientation_is_honoured() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path();
    let base = picture(3, 800, 600);
    fs::write(r.join("upright.jpg"), jpeg(&base, 92)).expect("w");
    // saved sideways with "rotate 90° clockwise to show"
    let sideways = imageops::rotate270(&base);
    fs::write(
        r.join("sideways.jpg"),
        with_orientation(&jpeg(&sideways, 92), 6),
    )
    .expect("w");
    // the same sideways pixels without the tag are another picture
    fs::write(r.join("untagged.jpg"), jpeg(&sideways, 92)).expect("w");
    let set = hashed(r, None);
    let sideways = set
        .photos
        .iter()
        .find(|p| p.path.ends_with("sideways.jpg"))
        .expect("hashed");
    assert_eq!((sideways.width, sideways.height), (800, 600));
    let rep = report(&set, Preset::Strict);
    assert_eq!(names(&rep, r), [vec!["sideways.jpg", "upright.jpg"]]);
}

#[test]
fn byte_identical_copies_are_marked_not_counted() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path();
    let base = picture(7, 640, 480);
    let full = jpeg(&base, 92);
    fs::create_dir_all(r.join("Pictures")).expect("mkdir");
    fs::create_dir_all(r.join("Downloads")).expect("mkdir");
    fs::write(r.join("Pictures/a.jpg"), &full).expect("w");
    fs::write(r.join("Downloads/a.jpg"), &full).expect("w");
    fs::write(
        r.join("Pictures/small.jpg"),
        jpeg(&resize(&base, 320, 240), 60),
    )
    .expect("w");
    let rep = report(&hashed(r, None), Preset::Strict);
    assert_eq!(rep.groups.len(), 1);
    let g = &rep.groups[0];
    // tie on pixels and size: the copy outside Downloads stays first
    assert!(g.photos[0].path.ends_with("Pictures/a.jpg"), "{g:?}");
    let copy = g
        .photos
        .iter()
        .find(|p| p.path.ends_with("Downloads/a.jpg"))
        .expect("copy");
    assert_eq!(
        copy.identical_to.as_deref(),
        Some(g.photos[0].path.as_path())
    );
    let small = g
        .photos
        .iter()
        .find(|p| p.path.ends_with("small.jpg"))
        .expect("small");
    // only the look-alike counts
    assert_eq!(g.extra, small.size);
    assert_eq!(rep.extra_bytes, small.size);
}

#[test]
fn the_cache_saves_the_second_run() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path();
    library(r);
    fs::write(
        r.join("broken.jpg"),
        &jpeg(&picture(9, 300, 200), 90)[..200],
    )
    .expect("w");
    let mut cache = MemoryImageCache::default();
    let first = hashed(r, Some(&mut cache));
    assert_eq!(first.cache_hits, 0);
    assert_eq!(first.decoded, 10);
    // file ids are known on NTFS, APFS and ext4: every picture is remembered,
    // the broken one too
    assert_eq!(cache.len(), 10);
    let second = hashed(r, Some(&mut cache));
    assert_eq!(second.cache_hits, 10);
    assert_eq!(second.decoded, 0);
    assert_eq!(second.skipped.get(&SimilarSkip::Unreadable), Some(&1));
    assert_eq!(
        names(&report(&second, Preset::Strict), r),
        names(&report(&first, Preset::Strict), r)
    );
    // a picture rewritten since is decoded again
    std::thread::sleep(std::time::Duration::from_millis(1100));
    fs::write(r.join("other.jpg"), jpeg(&picture(102, 800, 600), 92)).expect("w");
    let third = hashed(r, Some(&mut cache));
    assert_eq!(third.decoded, 1);
}

#[test]
fn hostile_files_are_skipped_without_harm() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path();
    let good = jpeg(&picture(5, 400, 300), 90);
    fs::write(r.join("cut.jpg"), &good[..150]).expect("w");
    fs::write(
        r.join("text.png"),
        b"definitely not a picture, just text in disguise",
    )
    .expect("w");
    // a valid JPEG whose header claims 20000 x 20000 pixels
    let mut huge = good.clone();
    let sof = huge
        .windows(2)
        .position(|w| w == [0xFF, 0xC0])
        .expect("baseline frame header");
    huge[sof + 5..sof + 9].copy_from_slice(&[0x4E, 0x20, 0x4E, 0x20]);
    assert_eq!(decode::decode(&huge).err(), Some(decode::Fail::TooLarge));
    fs::write(r.join("huge.jpg"), &huge).expect("w");
    // a plain colour has nothing to compare
    fs::write(
        r.join("plain.png"),
        png(&RgbImage::from_pixel(200, 200, image::Rgb([90, 90, 90]))),
    )
    .expect("w");
    let set = hashed(r, None);
    assert!(set.photos.is_empty(), "{:?}", set.photos);
    assert_eq!(set.skipped.get(&SimilarSkip::TooLarge), Some(&1));
    assert_eq!(set.skipped.get(&SimilarSkip::Plain), Some(&1));
    assert_eq!(set.skipped.get(&SimilarSkip::Unreadable), Some(&2));
}

#[test]
fn huge_declared_png_dimensions_are_refused_before_decoding() {
    // a 1 x 1 PNG whose header is rewritten to 100000 x 100000 (CRC fixed)
    let mut data = png(&RgbImage::from_pixel(1, 1, image::Rgb([1, 2, 3])));
    let ihdr = data.windows(4).position(|w| w == b"IHDR").expect("ihdr");
    data[ihdr + 4..ihdr + 8].copy_from_slice(&100_000u32.to_be_bytes());
    data[ihdr + 8..ihdr + 12].copy_from_slice(&100_000u32.to_be_bytes());
    let crc = crc32(&data[ihdr..ihdr + 17]);
    data[ihdr + 17..ihdr + 21].copy_from_slice(&crc.to_be_bytes());
    let err = decode::decode(&data).err();
    assert!(matches!(
        err,
        Some(decode::Fail::TooLarge | decode::Fail::Unreadable)
    ));
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[test]
fn small_files_and_other_types_are_not_pictures() {
    let dir = tempfile::tempdir().expect("tmp");
    let r = dir.path();
    let small = jpeg(&picture(4, 64, 48), 50);
    assert!((small.len() as u64) < SIMILAR_MIN_SIZE);
    fs::write(r.join("icon.jpg"), &small).expect("w");
    fs::write(r.join("notes.txt"), vec![b'x'; 50_000]).expect("w");
    let t = tree(r);
    let set = find_hashed(
        &[&t],
        &no_gate(),
        &SimilarOptions::default(),
        None,
        &LiveSource,
        &|_| {},
    )
    .expect("find");
    assert!(set.photos.is_empty());
    // the text file is not counted as a skipped picture
    assert_eq!(set.skipped.get(&SimilarSkip::Small), Some(&1));
    assert_eq!(set.skipped.values().sum::<u64>(), 1);
}

fn fake(path: &str, w: u32, h: u32, size: u64, mtime: i64) -> HashedPhoto {
    HashedPhoto {
        path: PathBuf::from(path),
        links: Vec::new(),
        size,
        mtime,
        volume: 1,
        file_id: None,
        flags: CopyFlags::default(),
        width: w,
        height: h,
        fingerprint: Fingerprint {
            fine: [0x5555_5555_5555_5555; 8],
            coarse: 0x00ff_00ff_00ff_00ff,
        },
        content: None,
    }
}

#[test]
fn the_best_photo_has_the_most_pixels_then_the_largest_file() {
    let set = HashedSet {
        photos: vec![
            fake("/p/b.jpg", 1600, 1200, 200_000, 10),
            fake("/p/a.jpg", 4000, 3000, 900_000, 20),
            fake("/p/c.jpg", 4000, 3000, 2_000_000, 30),
            fake("/home/Downloads/c.jpg", 4000, 3000, 2_000_000, 5),
        ],
        ..HashedSet::default()
    };
    let rep = report(&set, Preset::Strict);
    let order: Vec<&Path> = rep.groups[0]
        .photos
        .iter()
        .map(|p| p.path.as_path())
        .collect();
    // same pixels and size: Downloads goes after the rest, even though it is older
    assert_eq!(order[0], Path::new("/p/c.jpg"));
    assert_eq!(order[1], Path::new("/home/Downloads/c.jpg"));
    assert_eq!(order[2], Path::new("/p/a.jpg"));
    assert_eq!(order[3], Path::new("/p/b.jpg"));
}

#[test]
fn groups_do_not_chain() {
    // a ~ b and b ~ c, but a and c are too far apart: c is not pulled in through b
    let mut a = fake("/a.jpg", 1000, 1000, 3, 0);
    let mut b = fake("/b.jpg", 900, 900, 2, 0);
    let mut c = fake("/c.jpg", 800, 800, 1, 0);
    a.fingerprint.fine = [0; 8];
    b.fingerprint.fine = [0, 0, 0, 0, 0, 0, 0, (1 << 30) - 1];
    c.fingerprint.fine = [0, 0, 0, 0, 0, 0, (1 << 30) - 1, (1 << 30) - 1];
    let set = HashedSet {
        photos: vec![c, b, a],
        ..HashedSet::default()
    };
    let rep = report(&set, Preset::Strict);
    assert_eq!(rep.groups.len(), 1);
    let names: Vec<&Path> = rep.groups[0]
        .photos
        .iter()
        .map(|p| p.path.as_path())
        .collect();
    assert_eq!(names, [Path::new("/a.jpg"), Path::new("/b.jpg")]);
    assert_eq!(rep.groups[0].photos[1].distance, 30);
}

#[test]
fn presets_parse_and_widen() {
    assert_eq!("Normal".parse::<Preset>(), Ok(Preset::Normal));
    assert!("fuzzy".parse::<Preset>().is_err());
    assert_eq!(Preset::default(), Preset::Strict);
    assert!(Preset::Strict.max_distance() < Preset::Normal.max_distance());
    assert!(Preset::Normal.max_distance() < Preset::Loose.max_distance());
    assert!(Preset::Strict.max_coarse() < Preset::Loose.max_coarse());
}

fn group(dir: &Path) -> SimilarGroup {
    let photo = |name: &str, size: u64| Photo {
        path: dir.join(name),
        links: Vec::new(),
        size,
        mtime: 0,
        width: 10,
        height: 10,
        volume: 0,
        file_id: None,
        flags: CopyFlags::default(),
        distance: 0,
        identical_to: None,
    };
    SimilarGroup {
        id: 1,
        photos: vec![
            photo("best.jpg", 30),
            photo("b.jpg", 20),
            photo("c.jpg", 10),
        ],
        extra: 30,
    }
}

fn sel(remove: &[PathBuf], keep_none: bool) -> SimilarSelection {
    SimilarSelection {
        group: 1,
        remove: remove.to_vec(),
        keep_none,
    }
}

#[test]
fn the_planner_refuses_what_the_user_did_not_confirm() {
    let dir = tempfile::tempdir().expect("tmp");
    let d = dir.path();
    let g = vec![group(d)];
    let all = [d.join("best.jpg"), d.join("b.jpg"), d.join("c.jpg")];
    let gate = no_gate();

    // every photo without "keep none": refused
    assert_eq!(
        plan(&g, &[sel(&all, false)], Action::Recycle, &gate),
        Err(PlanError::NothingKept(1))
    );
    // ticked: allowed, one hand-picked item per photo
    let p = plan(&g, &[sel(&all, true)], Action::Recycle, &gate).expect("plan");
    assert_eq!(p.items.len(), 3);
    assert!(
        p.items
            .iter()
            .all(|i| i.origin == Origin::Manual && i.keeper.is_none())
    );
    // even the best photo may go when the user says so; sizes are kept for the gate
    let p = plan(&g, &[sel(&all[..1], false)], Action::Quarantine, &gate).expect("plan");
    assert_eq!(p.items[0].mode, DeleteMode::Quarantine);
    assert_eq!((p.items[0].size, p.items[0].bytes), (30, 30));

    assert_eq!(
        plan(
            &g,
            &[SimilarSelection {
                group: 9,
                ..sel(&all[..1], false)
            }],
            Action::Recycle,
            &gate
        ),
        Err(PlanError::UnknownGroup(9))
    );
    assert_eq!(
        plan(
            &g,
            &[sel(&all[..1], false), sel(&all[1..2], false)],
            Action::Recycle,
            &gate
        ),
        Err(PlanError::SelectedTwice(1))
    );
    assert!(matches!(
        plan(
            &g,
            &[sel(&[d.join("elsewhere.jpg")], false)],
            Action::Recycle,
            &gate
        ),
        Err(PlanError::NotInGroup { .. })
    ));
    // never replaced by a link to a look-alike
    assert!(matches!(
        plan(&g, &[sel(&all[1..2], false)], Action::Link, &gate),
        Err(PlanError::CannotLink { why: "similar", .. })
    ));
    // the gate's manual rules: nothing under ~/.ssh
    let home = d.to_string_lossy().replace('\\', "/");
    let strict = SafetyGate::new(&move |v: &str| (v == "HOME").then(|| home.clone()));
    let mut locked = group(d);
    locked.photos[2].path = d.join(".ssh").join("c.jpg");
    assert!(matches!(
        plan(
            &[locked.clone()],
            &[sel(&[locked.photos[2].path.clone()], false)],
            Action::Recycle,
            &strict
        ),
        Err(PlanError::Protected { .. })
    ));
}

#[test]
fn removal_checks_the_disk_right_before_each_photo() {
    let dir = tempfile::tempdir().expect("tmp");
    let d = dir.path();
    let mut g = group(d);
    for p in &mut g.photos {
        fs::write(&p.path, vec![1u8; usize::try_from(p.size).expect("small")]).expect("w");
        let info = LiveSource.info(&p.path).expect("info");
        p.mtime = info.mtime;
    }
    let groups = vec![g.clone()];
    let remove = [g.photos[1].path.clone(), g.photos[2].path.clone()];
    let p = plan(
        &groups,
        &[sel(&remove, false)],
        Action::Quarantine,
        &no_gate(),
    )
    .expect("plan");

    // the best photo stays and is unchanged: fine
    let mut v = SimilarVerifier::new(&LiveSource, &groups, &[sel(&remove, false)]);
    assert_eq!(v.check(&p.items[0]), Ok(()));
    // a photo outside the selection is never passed
    let mut other = p.items[0].clone();
    other.path = g.photos[0].path.clone();
    assert_eq!(v.check(&other).map_err(|r| r.code), Err("not-similar"));

    // the photo that stays went away meanwhile: nothing may go
    fs::remove_file(&g.photos[0].path).expect("rm");
    assert_eq!(
        v.check(&p.items[0]).map_err(|r| r.code),
        Err("keeper-changed")
    );
    // unless the user confirmed "keep none" for the group
    let mut none = SimilarVerifier::new(&LiveSource, &groups, &[sel(&remove, true)]);
    assert_eq!(none.check(&p.items[0]), Ok(()));

    // a photo rewritten since the search
    fs::write(&g.photos[1].path, b"edited").expect("w");
    assert_eq!(
        none.check(&p.items[0]).map_err(|r| r.code),
        Err("copy-changed")
    );

    // through the executor: the gate and the verifier together (quarantine)
    fs::write(&g.photos[0].path, vec![1u8; 30]).expect("w");
    let mtime = LiveSource.info(&g.photos[0].path).expect("info").mtime;
    let mut groups = groups;
    groups[0].photos[0].mtime = mtime;
    let mut v = SimilarVerifier::new(&LiveSource, &groups, &[sel(&remove, false)]);
    let opts = ExecOptions {
        op_id: exec::new_op_id(exec::unix_now()),
        kind: "similar".into(),
        owner: 0,
        roots: QuarantineRoots {
            system: Some(d.join("q")),
            other_volumes: false,
        },
        now: exec::unix_now(),
    };
    let op = exec::execute_verified(
        &p,
        &no_gate(),
        &LiveProbe,
        &opts,
        &mut |i| v.check(i),
        &mut |_, _, _| true,
    );
    // b.jpg changed since the search: refused; c.jpg goes to the quarantine
    assert_eq!(op.items[0].status, ItemStatus::Skipped);
    assert_eq!(op.items[1].status, ItemStatus::Done, "{:?}", op.items[1]);
    assert!(!g.photos[2].path.exists());
    assert!(g.photos[0].path.exists());
}
