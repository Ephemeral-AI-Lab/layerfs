use super::artifact::display_error;
use super::row_parse::{row_optional_u128, row_u128, ParsedRow};
use crate::stage1_fixture::EvalResult;
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AuthenticationValidation {
    pub(crate) applicable_rows: u64,
    pub(crate) fetched_authentication_failures: u64,
    pub(crate) fetched_role_decode_failures: u64,
    pub(crate) new_object_equation_failures: u64,
    pub(crate) incumbent_equation_failures: u64,
    pub(crate) payload_batch_maximum: u64,
}
pub(crate) fn validate_authentication(rows: &[ParsedRow]) -> EvalResult<AuthenticationValidation> {
    let mut result = AuthenticationValidation::default();
    for row in rows {
        let Some(fetched) = row_optional_u128(row, "fetched_rows")? else {
            continue;
        };
        result.applicable_rows += 1;
        let authentication = row_u128(row, "fetched_row_authentication_passes")?;
        let role_decode = row_u128(row, "fetched_row_role_decode_passes")?;
        let new_auth = row_u128(row, "new_object_authentication_passes")?;
        let created = row_u128(row, "created_rows")?;
        let reused = row_u128(row, "reused_rows")?;
        let put_lookup = row_u128(row, "put_lookup_statements")?;
        let put_insert = row_u128(row, "put_insert_statements")?;
        let incumbent = row_u128(row, "incumbent_authentication_passes")?;
        let objects_validated = row_u128(row, "objects_validated")?;
        let objects_created = row_u128(row, "objects_created")?;
        let objects_reused = row_u128(row, "objects_reused")?;
        let payload_max = row_u128(row, "payload_batch_maximum")?;
        let trusted = matches!(row.row_group.as_str(), "C02" | "C03" | "C05" | "C07");
        if (row.row_group == "C02" && authentication != 0)
            || (trusted && authentication > fetched)
            || (!trusted && fetched != authentication)
        {
            result.fetched_authentication_failures += 1;
        }
        if fetched != role_decode {
            result.fetched_role_decode_failures += 1;
        }
        if new_auth != created + reused || new_auth != put_lookup {
            result.new_object_equation_failures += 1;
        }
        if incumbent != reused {
            result.incumbent_equation_failures += 1;
        }
        if objects_validated != role_decode + new_auth + incumbent {
            return Err(format!(
                "{} objects_validated authentication equation",
                row.row_id
            ));
        }
        if put_insert != created || objects_created != created || objects_reused != reused {
            return Err(format!(
                "{} put_insert=created; objects_created=created; objects_reused=reused",
                row.row_id
            ));
        }
        let transaction = (
            row_u128(row, "transactions_started")?,
            row_u128(row, "transactions_committed")?,
            row_u128(row, "transactions_rolled_back")?,
            row_u128(row, "publication_transactions_started")?,
            row_u128(row, "publication_commits")?,
            row_u128(row, "publication_transactions_rolled_back")?,
        );
        if matches!(row.row_group.as_str(), "C03" | "C05" | "C07") {
            if transaction != (1, 1, 0, 1, 1, 0) {
                return Err(format!("{} one transition transaction/COMMIT", row.row_id));
            }
        } else if transaction != (0, 0, 0, 0, 0, 0) {
            return Err(format!("{} read-only transaction closure", row.row_id));
        }
        if row_u128(row, "admission_transactions_started")?
            != row_u128(row, "admission_transactions_committed")?
                + row_u128(row, "admission_transactions_rolled_back")?
            || row_u128(row, "integrity_transactions_started")?
                != row_u128(row, "integrity_transactions_committed")?
                    + row_u128(row, "integrity_transactions_rolled_back")?
        {
            return Err(format!(
                "{} admission/integrity transaction closure",
                row.row_id
            ));
        }
        result.payload_batch_maximum = result
            .payload_batch_maximum
            .max(u64::try_from(payload_max).map_err(display_error)?);
    }
    if result.fetched_authentication_failures != 0
        || result.fetched_role_decode_failures != 0
        || result.new_object_equation_failures != 0
        || result.incumbent_equation_failures != 0
        || result.payload_batch_maximum > 64
    {
        return Err(format!("row authentication closure failed: {result:?}"));
    }
    Ok(result)
}
pub(crate) fn json_array_objects<'a>(json: &'a str, key: &str) -> EvalResult<Vec<&'a str>> {
    let needle = format!("\"{key}\":[");
    let start = json
        .find(&needle)
        .map(|offset| offset + needle.len())
        .ok_or_else(|| format!("missing JSON array {key}"))?;
    let bytes = json.as_bytes();
    let mut objects = Vec::new();
    let mut depth = 0_u32;
    let mut object_start = None;
    let mut string = false;
    let mut escaped = false;
    for (relative, byte) in bytes[start..].iter().copied().enumerate() {
        if string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
            continue;
        }
        match byte {
            b'"' => string = true,
            b'{' => {
                if depth == 0 {
                    object_start = Some(start + relative);
                }
                depth += 1;
            }
            b'}' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| format!("JSON array {key} object underflow"))?;
                if depth == 0 {
                    let begin = object_start
                        .take()
                        .ok_or_else(|| format!("JSON array {key} missing object start"))?;
                    objects.push(&json[begin..start + relative + 1]);
                }
            }
            b']' if depth == 0 => return Ok(objects),
            _ => {}
        }
    }
    Err(format!("unterminated JSON array {key}"))
}
