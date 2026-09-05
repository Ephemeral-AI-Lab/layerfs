use layerfs_sdk::{BranchId, LayerStackStore};
use std::io::Write;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    run_with(&arguments, &mut std::io::stdout().lock())
}

fn run_with(
    arguments: &[String],
    output: &mut dyn Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let (store_path, branch_id, receipt_v1) = match arguments {
        [mode, store, id] if mode == "check" => (store, id.parse::<BranchId>()?, false),
        [mode, store, id, receipt] if mode == "check" && receipt == "--receipt-v1" => {
            (store, id.parse::<BranchId>()?, true)
        }
        _ => return Err("usage: layerfs-eval check <store-db> <branch-id> [--receipt-v1]".into()),
    };
    let store = LayerStackStore::connect(store_path)?;
    let pinned = store.pin_branch(branch_id)?;
    let reachable = store.reachable_root_storage(pinned.root)?;
    let success = if receipt_v1 {
        let head = pinned
            .branch
            .head_commit_id
            .map_or_else(|| "none".to_owned(), |id| id.to_string());
        receipt(
            &pinned.branch.id.to_string(),
            &pinned.layer_stack.id.to_string(),
            &pinned.branch.base_layer_id.to_string(),
            &head,
            &pinned.root.to_string(),
            reachable.objects,
            reachable.encoded_bytes,
        )
    } else {
        legacy(
            &format!("{:?}", pinned.branch.head_commit_id),
            &pinned.root.to_string(),
        )
    };
    output.write_all(success.as_bytes())?;
    Ok(())
}

fn receipt(
    branch: &str,
    stack: &str,
    base: &str,
    head: &str,
    root: &str,
    objects: u64,
    bytes: u64,
) -> String {
    format!(
        "layerfs_eval_receipt_version=1\nbranch_id={branch}\nlayer_stack_id={stack}\n\
         base_layer_id={base}\nhead_commit_id={head}\nroot_object_id={root}\n\
         reachable_objects={objects}\nreachable_encoded_bytes={bytes}\nstatus=ok\n"
    )
}

fn legacy(head: &str, root: &str) -> String {
    format!("{head} {root}\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use layerfs_sdk::{CommitId, LayerId, LayerStackId};

    #[test]
    fn default_output_is_unchanged_and_receipt_is_exact_and_repeatable() {
        let branch = BranchId::from_bytes([0x11; 17]).unwrap().to_string();
        let stack = LayerStackId::from_bytes([0x31; 17]).unwrap().to_string();
        let layer = LayerId::from_bytes([0x32; 33]).unwrap().to_string();
        let root = "00".repeat(32);
        let expected = format!(
            "layerfs_eval_receipt_version=1\nbranch_id={branch}\nlayer_stack_id={stack}\n\
             base_layer_id={layer}\nhead_commit_id=none\nroot_object_id={root}\n\
             reachable_objects=1\nreachable_encoded_bytes=13\nstatus=ok\n"
        );
        assert_eq!(legacy("None", &root), format!("None {root}\n"));
        assert_eq!(
            receipt(&branch, &stack, &layer, "none", &root, 1, 13),
            expected
        );
        assert_eq!(
            receipt(&branch, &stack, &layer, "none", &root, 1, 13),
            expected
        );
        assert!(strict_receipt(expected.as_bytes()));
        let mut out_of_order = expected.lines().collect::<Vec<_>>();
        out_of_order.swap(1, 2);
        let invalid = [
            expected.replacen("base_layer_id=", "branch_id=", 1),
            expected.replacen("status=ok", "result=ok", 1),
            expected.replacen(&format!("base_layer_id={layer}\n"), "", 1),
            format!("{}\n", out_of_order.join("\n")),
            expected.replacen(&branch, &"00".repeat(17), 1),
            expected.replacen(&root, &"g0".repeat(32), 1),
            expected.replacen("reachable_objects=1", "reachable_objects=01", 1),
            format!("{expected}trailing"),
        ];
        for value in invalid {
            assert!(!strict_receipt(value.as_bytes()), "accepted {value:?}");
        }
    }

    fn strict_receipt(bytes: &[u8]) -> bool {
        let Ok(text) = std::str::from_utf8(bytes) else {
            return false;
        };
        let Some(text) = text.strip_suffix('\n') else {
            return false;
        };
        let fields = text.split('\n').collect::<Vec<_>>();
        let [version, branch, stack, layer, head, root, objects, encoded_bytes, status] =
            fields.as_slice()
        else {
            return false;
        };
        let decimal = |field: &str, key: &str| {
            field
                .strip_prefix(key)
                .and_then(|value| value.parse::<u64>().ok().map(|number| (value, number)))
                .is_some_and(|(value, number)| number.to_string() == value)
        };
        *version == "layerfs_eval_receipt_version=1"
            && canonical_id::<BranchId>(branch, "branch_id=")
            && canonical_id::<LayerStackId>(stack, "layer_stack_id=")
            && canonical_id::<LayerId>(layer, "base_layer_id=")
            && (*head == "head_commit_id=none" || canonical_id::<CommitId>(head, "head_commit_id="))
            && root.strip_prefix("root_object_id=").is_some_and(|value| {
                value.len() == 64
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
            && decimal(objects, "reachable_objects=")
            && decimal(encoded_bytes, "reachable_encoded_bytes=")
            && *status == "status=ok"
    }

    fn canonical_id<T: std::str::FromStr + std::fmt::Display>(field: &str, key: &str) -> bool {
        field
            .strip_prefix(key)
            .and_then(|value| value.parse::<T>().ok().map(|id| (value, id)))
            .is_some_and(|(value, id)| id.to_string() == value)
    }
}
