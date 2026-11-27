//! Commit references: the text a caller names a Commit with, resolved in
//! the Store.
//!
//! A Commit is named by its full identity, 66 hex characters, or by a
//! prefix of 8 or more of them, which the catalog resolves in one read of
//! the project's identity index: no history is paged. A prefix that more
//! than one Commit starts with is refused `InvalidInput`, its detail naming
//! how many (`matches`). On main a publication is a Layer, so a Layer's
//! identity is read the same way. A reference (`<ref>`, `--from`) is a
//! Branch name first, and a Commit only when no Branch has that name.
use {super::lookup::{self as args, invalid, refuse}, layerfs_protocol::sdk::hex};
use super::MAIN;
use crate::store::Store;
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_content::ObjectId;
use layerfs_history::identity::{COMMIT_TAG, LAYER_TAG};
use layerfs_history::{
    BranchRecord, CommitRecord, HistoryName, LayerRecord, LayerStackRecord, NodeId,
};
use std::sync::Arc;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The characters of a full identity.
pub const FULL: usize = 66;
/// The fewest characters a prefix of an identity may have.
pub const PREFIX: usize = 8;

/// One saved node, read: a Commit, or a Layer main published.
#[derive(Clone, Debug)]
pub enum Saved {
    /// A Commit.
    Commit(Box<CommitRecord>),
    /// A Layer of main.
    Layer(Box<LayerRecord>),
}

impl Saved {
    /// Its identity.
    pub fn node(&self) -> NodeId {
        match self {
            Saved::Commit(record) => NodeId::Commit(record.id),
            Saved::Layer(record) => NodeId::Layer(record.id),
        }
    }

    /// Its identity as text: the full 66 characters.
    pub fn text(&self) -> String {
        args::node_text(self.node())
    }

    /// The root of the tree it records.
    pub fn root(&self) -> ObjectId {
        match self {
            Saved::Commit(record) => record.root,
            Saved::Layer(record) => record.root,
        }
    }

    /// The node it is compared with when no other side is named: a Commit's
    /// parent, else its base Layer; a Layer's parent, and none for the
    /// first Layer of main (the empty tree).
    pub fn before(&self) -> Option<NodeId> {
        match self {
            Saved::Commit(record) => {
                Some((record.parent).map_or(NodeId::Layer(record.base_layer), NodeId::Commit))
            }
            Saved::Layer(record) => record.parent.map(NodeId::Layer),
        }
    }
}

/// What a reference names.
#[derive(Clone, Debug)]
pub enum Named {
    /// `main`: the project's line of Layers.
    Main,
    /// A Branch.
    Branch(Box<BranchRecord>),
    /// A Commit, or a Layer of main, by its identity or a prefix of it.
    Node(Saved),
}

impl Named {
    /// The node it stands at: main's head Layer, a Branch's newest node,
    /// or the node itself.
    pub fn node(&self, stack: &LayerStackRecord) -> NodeId {
        match self {
            Named::Main => NodeId::Layer(stack.head_layer),
            Named::Branch(branch) => args::branch_node(branch),
            Named::Node(saved) => saved.node(),
        }
    }
}

fn no_commit(text: &str) -> SdkRefusal {
    refuse(
        "NoSuchCommit",
        format!("no Commit {text} in this project"),
        SdkValue::Null,
    )
}

/// The node `id` of `stack`, read; one of another project is not found.
pub fn saved(store: &Arc<Store>, stack: &LayerStackRecord, id: NodeId) -> Result<Saved> {
    let found = match id {
        NodeId::Commit(id) => {
            let record = args::commit(store, id)?;
            (record.stack == stack.id).then(|| Saved::Commit(Box::new(record)))
        }
        NodeId::Layer(id) => {
            let record = args::layer(store, id)?;
            (record.stack == stack.id).then(|| Saved::Layer(Box::new(record)))
        }
    };
    found.ok_or_else(|| no_commit(&args::node_text(id)))
}

/// The value of one hex digit.
fn digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

/// The lowest and highest identities that start with the hex `prefix`.
fn bounds(prefix: &str) -> Option<([u8; 33], [u8; 33])> {
    let (mut low, mut high) = ([0u8; 33], [0xffu8; 33]);
    for (at, c) in prefix.bytes().enumerate() {
        let value = digit(c)?;
        let (byte, shift) = (at / 2, if at % 2 == 0 { 4 } else { 0 });
        low[byte] |= value << shift;
        high[byte] &= !(0x0f << shift) | (value << shift);
    }
    Some((low, high))
}

/// The Commit of `stack` that `text` names: its full identity, or a prefix
/// of 8 or more of its characters, resolved by the catalog in one read. On
/// main a publication is a Layer, read the same way. Refused `InvalidInput`
/// when `text` is not such a text, and when more than one Commit starts
/// with it (the detail's `matches` is how many); `NoSuchCommit` when none
/// does.
pub fn commit(store: &Arc<Store>, stack: &LayerStackRecord, text: &str) -> Result<Saved> {
    let hex_text = text.bytes().all(|c| digit(c).is_some());
    if !hex_text || !(PREFIX..=FULL).contains(&text.len()) {
        return Err(invalid(format!(
            "`{text}` is not a Commit id: give its {FULL} characters, or a prefix of {PREFIX} or more"
        )));
    }
    if text.len() == FULL {
        return match args::node(text) {
            Ok(id) => saved(store, stack, id),
            Err(_) => Err(no_commit(text)),
        };
    }
    let (low, high) = bounds(text).ok_or_else(|| no_commit(text))?;
    if !matches!(low[0], COMMIT_TAG | LAYER_TAG) {
        return Err(no_commit(text));
    }
    let (first, matches) = store
        .published(|history| history.nodes_between(stack.id, &low, &high))
        .map_err(args::published)?;
    match (first, matches) {
        (Some(id), 1) => saved(store, stack, id),
        (Some(_), matches) => Err(refuse(
            "InvalidInput",
            format!("{matches} Commits start with {text}: give more of its characters"),
            SdkValue::map([(
                "matches",
                SdkValue::Int(i64::try_from(matches).unwrap_or(i64::MAX)),
            )]),
        )),
        (None, _) => Err(no_commit(text)),
    }
}

/// What `text` names as a reference: `main`, else the Branch of that name,
/// and only when no Branch has it a Commit as [`commit`] reads it.
pub fn reference(store: &Arc<Store>, stack: &LayerStackRecord, text: &str) -> Result<Named> {
    if text == MAIN {
        return Ok(Named::Main);
    }
    let missing = match HistoryName::new(text) {
        Ok(_) => match args::branch(store, stack, text) {
            Ok(branch) => return Ok(Named::Branch(Box::new(branch))),
            Err(refusal) if refusal.code == "NoSuchBranch" => Some(refusal),
            Err(refusal) => return Err(refusal),
        },
        Err(_) => None,
    };
    let id_shaped = text.len() >= PREFIX && text.bytes().all(|c| digit(c).is_some());
    match missing {
        // a name no Branch has, which is no identity either
        Some(refusal) if !id_shaped => Err(refusal),
        _ => commit(store, stack, text).map(Named::Node),
    }
}

/// The detail of a refusal for a head that is not the one expected: the
/// head it is at, for the caller's next call.
pub fn head_detail(head: NodeId) -> SdkValue {
    SdkValue::map([("head", SdkValue::text(hex(head.as_slice())))])
}

/// The node `text` names, which a write expects `head` to be: refused
/// `HeadMoved`, naming `head`, when it is another.
pub fn expect(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    text: Option<&str>,
    what: &str,
    head: NodeId,
) -> Result<()> {
    let Some(text) = text else {
        return Ok(());
    };
    let expected = commit(store, stack, text)?.node();
    if expected == head {
        return Ok(());
    }
    Err(refuse(
        "HeadMoved",
        format!(
            "{what} is at {}, not the expected {}",
            args::node_text(head),
            args::node_text(expected)
        ),
        head_detail(head),
    ))
}
