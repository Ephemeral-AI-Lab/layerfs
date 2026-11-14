//! The AppleDouble image codec on its own: what a macOS client's images
//! decode to, and that a decoded one encodes back to the very same bytes.
use layerfs_nfs::apple_double_codec::{Attribute, Fork, Image, IMAGE_UNIT};

const FILLER_AT: usize = 8;
const COUNT_AT: usize = 24;
const FINDER_TAG_AT: usize = 26;
const FINDER_ENTRY_AT: usize = 30;
const FINDER_ENTRY_LEN_AT: usize = 34;
const FORK_TAG_AT: usize = 38;
const FORK_AT: usize = 42;
const FORK_LEN_AT: usize = 46;
const FINDER_AT: usize = 50;
const ATTR_AT: usize = 84;
const ATTR_TAG_AT: usize = ATTR_AT + 4;
const ATTR_FORK_AT: usize = ATTR_AT + 8;
const ATTR_DATA_AT: usize = ATTR_AT + 12;
const ATTR_DATA_LEN_AT: usize = ATTR_AT + 16;
const ATTR_COUNT_AT: usize = ATTR_AT + 34;
const ENTRIES_AT: usize = ATTR_AT + 36;

fn pattern(seed: u32, length: usize) -> Vec<u8> {
    (0..length)
        .map(|n| (n as u32).wrapping_mul(31).wrapping_add(seed * 97) as u8)
        .collect()
}
fn image(finder: [u8; 32], tag: u32, attrs: &[(&[u8], Vec<u8>)]) -> Image {
    let attrs = attrs
        .iter()
        .map(|(name, value)| Attribute::new(name, value))
        .collect();
    Image::new(finder, tag, attrs).expect("attributes an entry can describe")
}
fn get32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
}
fn get16(bytes: &[u8], at: usize) -> u16 {
    u16::from_be_bytes(bytes[at..at + 2].try_into().unwrap())
}
/// The decoded form of `image`, required to encode back to the same bytes.
fn whole(image: &Image) -> Vec<u8> {
    let bytes = image.encode();
    let decoded = Image::compact(&bytes).expect("a whole image decodes");
    assert_eq!(decoded.encode(), bytes, "decoded form re-encodes the image");
    assert_eq!(decoded.finder_info(), image.finder_info());
    assert_eq!(decoded.fork(), image.fork());
    assert_eq!(decoded.len(), bytes.len());
    bytes
}
/// Where the first attribute entry's name ends: its NUL byte.
fn first_name_end(name_len: usize) -> usize {
    ENTRIES_AT + 11 + name_len
}

#[test]
fn values_of_every_size_round_trip() {
    for size in [0, 1, 4096, 64 * 1024, 100 * 1024] {
        let value = pattern(3, size);
        let name = format!("user.size-{size}");
        let attrs: &[(&[u8], Vec<u8>)] = &[(name.as_bytes(), value)];
        let built = image([1; 32], 0, attrs);
        let bytes = whole(&built);
        assert_eq!(
            bytes.len() % IMAGE_UNIT,
            0,
            "an image is whole {IMAGE_UNIT}-byte units"
        );
        assert!(bytes.len() >= size);
        let decoded = Image::compact(&bytes).unwrap();
        assert_eq!(decoded.attributes()[0].value.len(), size);
        assert_eq!(&decoded.attributes()[0].value, &attrs[0].1);
        assert_eq!(&decoded.attributes()[0].name, &attrs[0].0);
        assert!(decoded.cost() >= size);
    }
}

#[test]
fn many_attributes_and_finder_info_round_trip() {
    let finder = [9u8; 32];
    let attrs: Vec<(Vec<u8>, Vec<u8>)> = (0..12)
        .map(|n| {
            (
                format!("user.attr-{n:02}").into_bytes(),
                pattern(n, 7 + 13 * n as usize),
            )
        })
        .collect();
    let mut with_std = vec![
        (b"com.apple.provenance".to_vec(), pattern(21, 40)),
        (b"com.apple.quarantine".to_vec(), pattern(22, 150)),
        (b"com.apple.FinderInfo".to_vec(), finder.to_vec()),
    ];
    with_std.extend(attrs);
    let borrowed: Vec<(&[u8], Vec<u8>)> = with_std
        .iter()
        .map(|(name, value)| (name.as_slice(), value.clone()))
        .collect();
    let built = image(finder, 0x2a2a_2a2a, &borrowed);
    let bytes = whole(&built);
    let decoded = Image::compact(&bytes).unwrap();
    assert_eq!(decoded.finder_info(), finder);
    let names: Vec<Vec<u8>> = decoded
        .attributes()
        .iter()
        .map(|attr| attr.name.clone())
        .collect();
    let expected: Vec<Vec<u8>> = with_std.iter().map(|(name, _)| name.clone()).collect();
    assert_eq!(names, expected, "entry order is kept");
    for (attr, (_, value)) in decoded.attributes().iter().zip(&with_std) {
        assert_eq!(&attr.value, value);
    }
}

#[test]
fn the_clients_alignment_padding_is_kept() {
    // A client that rewrote a shorter name does not clear what the longer one
    // left behind: the bytes after the NUL stay, and the image must still
    // decode to itself.
    let attrs: &[(&[u8], Vec<u8>)] = &[
        (b"com.example.k4", pattern(5, 32)),
        (b"user.other", pattern(6, 8)),
    ];
    let mut bytes = image([2; 32], 0, attrs).encode();
    let name_end = first_name_end(b"com.example.k4".len());
    bytes[name_end + 1] = 0x5c;
    bytes[name_end + 2] = 0x00;
    let decoded = Image::compact(&bytes).expect("kept padding still decodes");
    assert_eq!(
        decoded.encode(),
        bytes,
        "the padding is part of the decoded image"
    );
    // A byte the layout leaves unused is not: such an image stays as written.
    let mut foreign = bytes.clone();
    let fork_at = get32(&foreign, FORK_AT) as usize;
    let data_len = get32(&foreign, ATTR_DATA_LEN_AT) as usize;
    let data_start = get32(&foreign, ATTR_DATA_AT) as usize;
    foreign[data_start + data_len..fork_at].fill(0x7f);
    assert!(
        Image::compact(&foreign).is_none(),
        "a byte the decoded form does not keep is not claimed"
    );
    assert!(
        Image::parse(&foreign).is_some(),
        "the fields still parse, only the round trip does not hold"
    );
}

#[test]
fn a_resource_fork_is_left_as_the_client_wrote_it() {
    // The client's own empty fork is part of the layout and decodes.
    let built = image([3; 32], 0, &[(b"user.a", pattern(7, 16))]);
    let bytes = whole(&built);
    assert_eq!(Image::compact(&bytes).unwrap().fork(), Fork::Blank);
    // The same entry holding other bytes is a real resource fork: the body is
    // kept as written, so no fork byte is lost.
    let mut fork_data = bytes.clone();
    fork_data[bytes.len() - 286..].fill(0x5a);
    assert!(Image::parse(&fork_data).is_some());
    assert!(Image::compact(&fork_data).is_none());
    // A fork of another length is not the layout this codec describes.
    let mut longer = bytes.clone();
    put32(&mut longer, FORK_LEN_AT, 300);
    assert!(Image::parse(&longer).is_none());
    // An image with a zero-length fork entry decodes too.
    let fork_at = get32(&bytes, FORK_AT) as usize;
    let mut bare = bytes[..fork_at].to_vec();
    put32(&mut bare, FORK_LEN_AT, 0);
    let decoded = Image::compact(&bare).expect("a fork-free image decodes");
    assert_eq!(decoded.fork(), Fork::None);
    assert_eq!(decoded.encode(), bare);
}

#[test]
fn malformed_images_are_not_decoded() {
    let good = image(
        [4; 32],
        0,
        &[(b"user.a", pattern(8, 32)), (b"user.b", pattern(9, 4))],
    )
    .encode();
    let fork_at = get32(&good, FORK_AT) as usize;
    let data_start = get32(&good, ATTR_DATA_AT) as usize;
    let mut cases: Vec<(&str, Vec<u8>)> = vec![
        ("empty", Vec::new()),
        ("one byte", vec![0]),
        ("a truncated header", good[..ENTRIES_AT - 1].to_vec()),
        ("a truncated value", good[..data_start + 4].to_vec()),
        ("a truncated image", good[..good.len() - 1].to_vec()),
        ("an extended image", [good.clone(), vec![0]].concat()),
    ];
    let mut each = |name: &'static str, at: usize, value: u32| {
        let mut bytes = good.clone();
        put32(&mut bytes, at, value);
        cases.push((name, bytes));
    };
    each("another magic", 0, 0x0005_1608);
    each("another version", 4, 0x0003_0000);
    each("another entry count", COUNT_AT, 3);
    each("another FinderInfo tag", FINDER_TAG_AT, 8);
    each("another FinderInfo offset", FINDER_ENTRY_AT, 51);
    each("another FinderInfo length", FINDER_ENTRY_LEN_AT, 32);
    each("another fork tag", FORK_TAG_AT, 1);
    each(
        "a fork before the entries",
        FORK_AT,
        (ENTRIES_AT - 1) as u32,
    );
    each("a fork past the image", FORK_AT, (good.len() + 1) as u32);
    each("another fork length", FORK_LEN_AT, 285);
    each("another ATTR magic", ATTR_AT, 0x4154_5453);
    each("another ATTR fork", ATTR_FORK_AT, (fork_at - 1) as u32);
    each(
        "a data start past the fork",
        ATTR_DATA_AT,
        (fork_at + 1) as u32,
    );
    each("a longer data length", ATTR_DATA_LEN_AT, 1024);
    each("another entry count", ATTR_COUNT_AT, 3);
    each("another value offset", ENTRIES_AT, (data_start + 1) as u32);
    let mut filler = good.clone();
    filler[FILLER_AT..FILLER_AT + 16].copy_from_slice(b"Mac OS          ");
    cases.push(("another filler", filler));
    let mut unterminated = good.clone();
    *unterminated
        .get_mut(first_name_end(b"user.a".len()))
        .unwrap() = b'X';
    cases.push(("a name without its NUL", unterminated));
    let mut nameless = good.clone();
    let name_len_at = ENTRIES_AT + 10;
    assert_eq!(nameless[name_len_at], (b"user.a".len() + 1) as u8);
    nameless[name_len_at] = 1;
    cases.push(("an empty name", nameless));
    for (name, bytes) in cases {
        assert!(
            Image::compact(&bytes).is_none(),
            "{name} is not claimed as a whole image"
        );
    }
}

#[test]
fn an_image_the_client_would_write_has_whole_units() {
    let built = image([6; 32], 0, &[]);
    let bytes = built.encode();
    assert_eq!(bytes.len() % IMAGE_UNIT, 0);
    assert_eq!(bytes.len(), IMAGE_UNIT, "the smallest image is one unit");
    assert_eq!(get16(&bytes, COUNT_AT), 2);
    assert_eq!(get32(&bytes, ATTR_TAG_AT), 0);
    assert_eq!(
        get32(&bytes, ATTR_DATA_AT) as usize,
        ENTRIES_AT,
        "no attribute, no entry"
    );
    assert_eq!(bytes[FINDER_AT..FINDER_AT + 32], [6; 32]);
    // An image whose FinderInfo the client filled and whose tag is its own.
    let tagged = image([7; 32], 0x1234_5678, &[]);
    let bytes = whole(&tagged);
    assert_eq!(get32(&bytes, ATTR_TAG_AT), 0x1234_5678);
}
