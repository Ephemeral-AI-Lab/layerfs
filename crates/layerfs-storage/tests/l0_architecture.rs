mod support;

fn assert_path_absent(path: &std::path::Path) {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(metadata) => {
            panic!("expected {path:?} to be absent, but metadata succeeded: {metadata:?}")
        }
        Err(error) => panic!("expected {path:?} to be absent, but lookup failed: {error}"),
    }
}

fn section<'a>(manifest: &'a str, name: &str) -> &'a str {
    let header = format!("[{name}]");
    let Some(header_start) = manifest.find(&header) else {
        return "";
    };
    let start = header_start + header.len();
    let rest = &manifest[start..];
    rest.find("\n[").map(|end| &rest[..end]).unwrap_or(rest)
}

fn relative_files(root: &std::path::Path) -> Vec<String> {
    fn visit(root: &std::path::Path, current: &std::path::Path, files: &mut Vec<String>) {
        let mut entries = std::fs::read_dir(current)
            .unwrap_or_else(|error| panic!("read directory {current:?}: {error}"))
            .map(|entry| entry.unwrap_or_else(|error| panic!("read directory entry: {error}")))
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            if entry
                .file_type()
                .unwrap_or_else(|error| panic!("read file type for {path:?}: {error}"))
                .is_dir()
            {
                visit(root, &path, files);
            } else {
                files.push(
                    path.strip_prefix(root)
                        .unwrap_or_else(|error| panic!("strip {path:?}: {error}"))
                        .to_string_lossy()
                        .replace(std::path::MAIN_SEPARATOR, "/"),
                );
            }
        }
    }

    let mut files = Vec::new();
    visit(root, root, &mut files);
    files.sort();
    files
}

fn cargo_test_targets(manifest: &str) -> Vec<(String, String, Option<String>)> {
    let mut blocks = Vec::new();
    let mut current = None;
    for line in manifest.lines() {
        if line.trim() == "[[test]]" {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            current = Some(String::new());
        } else if line.starts_with("[[") || line.starts_with("[") {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
        } else if let Some(block) = current.as_mut() {
            block.push_str(line);
            block.push('\n');
        }
    }
    if let Some(block) = current {
        blocks.push(block);
    }

    fn field(block: &str, name: &str) -> String {
        block
            .lines()
            .find_map(|line| {
                let (key, value) = line.trim().split_once('=')?;
                (key.trim() == name).then(|| value.trim().trim_matches('"').to_owned())
            })
            .unwrap_or_default()
    }

    blocks
        .into_iter()
        .filter(|block| !block.trim().is_empty())
        .map(|block| {
            let required_features = block.lines().find_map(|line| {
                let (key, value) = line.trim().split_once('=')?;
                (key.trim() == "required-features").then(|| value.trim().to_owned())
            });
            (
                field(&block, "name"),
                field(&block, "path"),
                required_features,
            )
        })
        .collect()
}

fn source_char_literal_start(bytes: &[u8], index: usize) -> bool {
    if bytes.get(index) != Some(&b'\'') {
        return false;
    }
    let Some(&next) = bytes.get(index + 1) else {
        return false;
    };
    next == b'\\'
        || bytes.get(index + 2) == Some(&b'\'')
        || (!next.is_ascii_alphanumeric() && next != b'_')
}

fn function_body<'a>(source: &'a str, name: &str) -> &'a str {
    let signature = format!("fn {name}");
    let start = source
        .find(&signature)
        .unwrap_or_else(|| panic!("missing function {name}"));
    let open = source[start..]
        .find('{')
        .map(|offset| start + offset)
        .unwrap_or_else(|| panic!("function {name} has no body"));
    let bytes = source.as_bytes();
    let mut depth = 0_u32;
    let mut index = open;
    let mut line_comment = false;
    let mut block_comment_depth = 0_u32;
    let mut string = false;
    let mut character = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if line_comment {
            if byte == b'\n' {
                line_comment = false;
            }
            index += 1;
            continue;
        }
        if block_comment_depth != 0 {
            if bytes.get(index..index + 2) == Some(b"/*") {
                block_comment_depth += 1;
                index += 2;
            } else if bytes.get(index..index + 2) == Some(b"*/") {
                block_comment_depth -= 1;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
            index += 1;
            continue;
        }
        if character {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'\'' {
                character = false;
            }
            index += 1;
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"//") {
            line_comment = true;
            index += 2;
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"/*") {
            block_comment_depth = 1;
            index += 2;
            continue;
        }
        match byte {
            b'"' => string = true,
            b'\'' if source_char_literal_start(bytes, index) => character = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &source[open + 1..index];
                }
            }
            _ => {}
        }
        index += 1;
    }
    panic!("unterminated function {name}");
}

fn skip_source_trivia(source: &str, mut index: usize) -> usize {
    let bytes = source.as_bytes();
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
        } else if bytes.get(index..index + 2) == Some(b"//") {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
        } else if bytes.get(index..index + 2) == Some(b"/*") {
            index += 2;
            let mut depth = 1_u32;
            while index < bytes.len() && depth != 0 {
                if bytes.get(index..index + 2) == Some(b"/*") {
                    depth += 1;
                    index += 2;
                } else if bytes.get(index..index + 2) == Some(b"*/") {
                    depth -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
            }
        } else {
            break;
        }
    }
    index
}

fn source_attribute_end(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut index = start;
    let mut depth = 0_u32;
    let mut string = false;
    let mut character = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
            index += 1;
            continue;
        }
        if character {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'\'' {
                character = false;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' => string = true,
            b'\'' if source_char_literal_start(bytes, index) => character = true,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
            }
            _ => {}
        }
        index += 1;
    }
    panic!("unterminated source attribute");
}

fn cfg_test_item_end(source: &str, attribute_end: usize) -> usize {
    let mut item_start = skip_source_trivia(source, attribute_end);
    while source.as_bytes().get(item_start..item_start + 2) == Some(b"#[") {
        item_start = source_attribute_end(source, item_start + 1);
        item_start = skip_source_trivia(source, item_start);
    }

    let bytes = source.as_bytes();
    let mut index = item_start;
    let mut braces = 0_u32;
    let mut parentheses = 0_u32;
    let mut brackets = 0_u32;
    let mut line_comment = false;
    let mut block_comment_depth = 0_u32;
    let mut string = false;
    let mut character = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if line_comment {
            if byte == b'\n' {
                line_comment = false;
            }
            index += 1;
            continue;
        }
        if block_comment_depth != 0 {
            if bytes.get(index..index + 2) == Some(b"/*") {
                block_comment_depth += 1;
                index += 2;
            } else if bytes.get(index..index + 2) == Some(b"*/") {
                block_comment_depth -= 1;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
            index += 1;
            continue;
        }
        if character {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'\'' {
                character = false;
            }
            index += 1;
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"//") {
            line_comment = true;
            index += 2;
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"/*") {
            block_comment_depth = 1;
            index += 2;
            continue;
        }
        match byte {
            b'"' => string = true,
            b'\'' if source_char_literal_start(bytes, index) => character = true,
            b'(' => parentheses += 1,
            b')' => parentheses -= 1,
            b'[' => brackets += 1,
            b']' => brackets -= 1,
            b'{' => braces += 1,
            b'}' => {
                braces -= 1;
                if braces == 0 && parentheses == 0 && brackets == 0 {
                    let after_body = skip_source_trivia(source, index + 1);
                    return if bytes.get(after_body) == Some(&b';') {
                        after_body + 1
                    } else {
                        index + 1
                    };
                }
            }
            b';' if braces == 0 && parentheses == 0 && brackets == 0 => return index + 1,
            _ => {}
        }
        index += 1;
    }
    panic!(
        "unterminated cfg(test) item at {item_start}: {}",
        source[item_start..]
            .lines()
            .take(3)
            .collect::<Vec<_>>()
            .join(" ")
    );
}

fn final_test_segments(source: &str) -> Vec<(String, usize, &str)> {
    let starts = source
        .match_indices("#[test]")
        .filter_map(|(index, _)| {
            let line_start = source[..index].rfind('\n').map_or(0, |offset| offset + 1);
            let line_end = source[index..]
                .find('\n')
                .map_or(source.len(), |offset| index + offset);
            (source[line_start..line_end].trim() == "#[test]").then_some(index)
        })
        .collect::<Vec<_>>();

    starts
        .iter()
        .enumerate()
        .map(|(index, &start)| {
            // Frozen PB-08 fingerprints deliberately retain the historical
            // `#[test]`-to-next-`#[test]` source segmentation.
            let end = starts.get(index + 1).copied().unwrap_or(source.len());
            let after_attribute = source[start..]
                .find('\n')
                .map_or(start + "#[test]".len(), |offset| start + offset + 1);
            let name = source[after_attribute..end]
                .lines()
                .filter_map(|line| {
                    let trimmed = line.trim();
                    if trimmed.starts_with("#[") {
                        return None;
                    }
                    let rest = trimmed.strip_prefix("fn ")?;
                    Some(
                        rest.split_once('(')
                            .map(|(name, _)| name.trim().to_owned())
                            .unwrap_or_else(|| {
                                panic!("test attribute at byte {start} has no function name")
                            }),
                    )
                })
                .next()
                .unwrap_or_else(|| panic!("test attribute at byte {start} has no function"));
            (name, start, &source[start..end])
        })
        .collect()
}

fn enclosing_module_source(source: &str, item_start: usize) -> &str {
    let mut enclosing = source;
    for (start, _) in source[..item_start].match_indices("mod ") {
        let line_start = source[..start].rfind('\n').map_or(0, |index| index + 1);
        if !matches!(source[line_start..start].trim(), "" | "pub") {
            continue;
        }
        let tail = &source[start..];
        let Some(open) = tail.find('{').map(|offset| start + offset) else {
            continue;
        };
        if tail
            .find(';')
            .is_some_and(|semicolon| start + semicolon < open)
        {
            continue;
        }
        let Some(end) = brace_end(source, open) else {
            continue;
        };
        if item_start < end && end - start < enclosing.len() {
            enclosing = &source[start..end];
        }
    }
    enclosing
}

fn scoped_function_definitions(
    source: &str,
    item_start: usize,
    base: &std::collections::BTreeMap<String, Vec<String>>,
) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut definitions = base.clone();
    definitions.extend(semantic_function_definitions(&[enclosing_module_source(
        source, item_start,
    )
    .to_owned()]));
    definitions
}

fn feature_gated_test_names(source: &str) -> std::collections::BTreeSet<String> {
    let declarations = final_test_segments(source);
    let mut gated = std::collections::BTreeSet::new();
    for (attribute_start, _) in source.match_indices("#[cfg(feature = \"operation-polymorphism\")]")
    {
        let attribute_end = source_attribute_end(source, attribute_start + 1);
        let item_end = cfg_test_item_end(source, attribute_end);
        for (name, test_start, _) in &declarations {
            if *test_start > attribute_start && *test_start < item_end {
                gated.insert(name.clone());
            }
        }
    }
    gated
}

fn normalized_semantic_source(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut normalized = String::with_capacity(source.len());
    let mut index = 0_usize;
    let mut block_comment_depth = 0_u32;
    let mut line_comment = false;
    let mut string = false;
    let mut character = false;
    let mut escaped = false;
    while index < bytes.len() {
        if line_comment {
            if bytes[index] == b'\n' {
                line_comment = false;
            }
            index += 1;
            continue;
        }
        if block_comment_depth != 0 {
            if bytes.get(index..index + 2) == Some(b"/*") {
                block_comment_depth += 1;
                index += 2;
            } else if bytes.get(index..index + 2) == Some(b"*/") {
                block_comment_depth -= 1;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if string || character {
            let byte = bytes[index];
            normalized.push(char::from(byte));
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if (string && byte == b'"') || (character && byte == b'\'') {
                string = false;
                character = false;
            }
            index += 1;
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"//") {
            line_comment = true;
            index += 2;
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"/*") {
            block_comment_depth = 1;
            index += 2;
            continue;
        }
        let byte = bytes[index];
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        normalized.push(char::from(byte));
        if byte == b'"' {
            string = true;
        } else if byte == b'\'' && source_char_literal_start(bytes, index) {
            character = true;
        }
        index += 1;
    }
    normalized
}

fn assertion_macro_sequence(source: &str) -> Vec<String> {
    const ASSERTIONS: [&str; 7] = [
        "assert",
        "assert_eq",
        "assert_ne",
        "debug_assert",
        "debug_assert_eq",
        "debug_assert_ne",
        "matches",
    ];

    let bytes = source.as_bytes();
    let mut sequence = Vec::new();
    let mut index = 0_usize;
    let mut line_comment = false;
    let mut block_comment_depth = 0_u32;
    let mut string = false;
    let mut character = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if line_comment {
            line_comment = byte != b'\n';
            index += 1;
            continue;
        }
        if block_comment_depth != 0 {
            if bytes.get(index..index + 2) == Some(b"/*") {
                block_comment_depth += 1;
                index += 2;
            } else if bytes.get(index..index + 2) == Some(b"*/") {
                block_comment_depth -= 1;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if string || character {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if (string && byte == b'"') || (character && byte == b'\'') {
                string = false;
                character = false;
            }
            index += 1;
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"//") {
            line_comment = true;
            index += 2;
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"/*") {
            block_comment_depth = 1;
            index += 2;
            continue;
        }
        if byte == b'"' {
            string = true;
            index += 1;
            continue;
        }
        if byte == b'\'' && source_char_literal_start(bytes, index) {
            character = true;
            index += 1;
            continue;
        }
        if byte.is_ascii_alphabetic() || byte == b'_' {
            let start = index;
            index += 1;
            while bytes
                .get(index)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            {
                index += 1;
            }
            let name = &source[start..index];
            let bang = skip_source_trivia(source, index);
            let open = skip_source_trivia(source, bang.saturating_add(1));
            if ASSERTIONS.contains(&name)
                && bytes.get(bang) == Some(&b'!')
                && bytes.get(open) == Some(&b'(')
            {
                sequence.push(format!("{name}!("));
            }
            continue;
        }
        index += 1;
    }
    sequence
}

fn assertion_expressions(source: &str) -> Vec<String> {
    const ASSERTIONS: [&str; 7] = [
        "assert",
        "assert_eq",
        "assert_ne",
        "debug_assert",
        "debug_assert_eq",
        "debug_assert_ne",
        "matches",
    ];

    let source = normalized_semantic_source(source);
    let bytes = source.as_bytes();
    let mut expressions = Vec::new();
    let mut index = 0_usize;
    while index < bytes.len() {
        if !bytes[index].is_ascii_alphabetic() && bytes[index] != b'_' {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            index += 1;
        }
        let name = &source[start..index];
        if !ASSERTIONS.contains(&name)
            || bytes.get(index) != Some(&b'!')
            || bytes.get(index + 1) != Some(&b'(')
        {
            continue;
        }

        let mut cursor = index + 2;
        let mut parentheses = 1_u32;
        let mut brackets = 0_u32;
        let mut braces = 0_u32;
        let mut string = false;
        let mut character = false;
        let mut escaped = false;
        while cursor < bytes.len() {
            let byte = bytes[cursor];
            if string || character {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if (string && byte == b'"') || (character && byte == b'\'') {
                    string = false;
                    character = false;
                }
                cursor += 1;
                continue;
            }
            match byte {
                b'"' => string = true,
                b'\'' if source_char_literal_start(bytes, cursor) => character = true,
                b'(' => parentheses += 1,
                b')' => {
                    parentheses -= 1;
                    if parentheses == 0 && brackets == 0 && braces == 0 {
                        let expression = &source[start..=cursor];
                        if !expression.starts_with("assert!(matches!(") {
                            expressions.push(expression.to_owned());
                        }
                        // Keep scanning the assertion body so nested `matches!`
                        // obligations remain visible, as they are in the frozen
                        // inventory sequence.
                        index += 2;
                        break;
                    }
                }
                b'[' => brackets += 1,
                b']' => brackets = brackets.saturating_sub(1),
                b'{' => braces += 1,
                b'}' => braces = braces.saturating_sub(1),
                _ => {}
            }
            cursor += 1;
        }
        assert!(
            cursor < bytes.len(),
            "unterminated assertion expression: {}",
            &source[start..]
        );
    }
    expressions
}

fn assertion_arguments(assertion: &str) -> Vec<&str> {
    let open = assertion
        .find('(')
        .expect("assertion has opening delimiter");
    let inner = &assertion[open + 1..assertion.len() - 1];
    let mut arguments = Vec::new();
    let bytes = inner.as_bytes();
    let mut start = 0_usize;
    let mut parentheses = 0_u32;
    let mut brackets = 0_u32;
    let mut braces = 0_u32;
    let mut string = false;
    let mut character = false;
    let mut escaped = false;
    for (index, byte) in bytes.iter().copied().enumerate() {
        if string || character {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if (string && byte == b'"') || (character && byte == b'\'') {
                string = false;
                character = false;
            }
            continue;
        }
        match byte {
            b'"' => string = true,
            b'\'' if source_char_literal_start(bytes, index) => character = true,
            b'(' => parentheses += 1,
            b')' => parentheses = parentheses.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            b',' if parentheses == 0 && brackets == 0 && braces == 0 => {
                arguments.push(&inner[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    arguments.push(&inner[start..]);
    arguments
}

fn call_arguments_at(source: &str, open: usize) -> Option<(Vec<&str>, usize)> {
    let bytes = source.as_bytes();
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    let mut arguments = Vec::new();
    let mut start = open + 1;
    let mut parentheses = 0_u32;
    let mut brackets = 0_u32;
    let mut braces = 0_u32;
    let mut string = false;
    let mut character = false;
    let mut escaped = false;
    let mut index = start;
    while index < bytes.len() {
        let byte = bytes[index];
        if string || character {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if (string && byte == b'"') || (character && byte == b'\'') {
                string = false;
                character = false;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' => string = true,
            b'\'' if source_char_literal_start(bytes, index) => character = true,
            b'(' => parentheses += 1,
            b')' if parentheses == 0 && brackets == 0 && braces == 0 => {
                if !source[start..index].trim().is_empty() {
                    arguments.push(&source[start..index]);
                }
                return Some((arguments, index + 1));
            }
            b')' => parentheses -= 1,
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            b',' if parentheses == 0 && brackets == 0 && braces == 0 => {
                arguments.push(&source[start..index]);
                start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }
    None
}

fn brace_end(source: &str, open: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(open) != Some(&b'{') {
        return None;
    }
    let mut depth = 0_u32;
    let mut string = false;
    let mut character = false;
    let mut escaped = false;
    for index in open..bytes.len() {
        let byte = bytes[index];
        if string || character {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if (string && byte == b'"') || (character && byte == b'\'') {
                string = false;
                character = false;
            }
            continue;
        }
        match byte {
            b'"' => string = true,
            b'\'' if source_char_literal_start(bytes, index) => character = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
    }
    None
}

fn assertion_expected_atoms(assertion: &str) -> Vec<String> {
    let arguments = assertion_arguments(assertion);
    let name = &assertion[..assertion.find('!').expect("assertion macro bang")];
    let expected = match name {
        "assert_eq" | "assert_ne" | "debug_assert_eq" | "debug_assert_ne" | "matches" => {
            arguments.get(1).copied().unwrap_or_default()
        }
        _ => arguments.first().copied().unwrap_or_default(),
    };
    let bytes = expected.as_bytes();
    let mut atoms = Vec::new();
    let mut index = 0_usize;
    while index < bytes.len() {
        if bytes[index].is_ascii_digit() {
            if bytes.get(index.wrapping_sub(1)) == Some(&b'.') {
                index += 1;
                continue;
            }
            let start = index;
            index += 1;
            while bytes
                .get(index)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'.'))
            {
                index += 1;
            }
            atoms.push(expected[start..index].to_owned());
            continue;
        }
        if bytes[index] == b'"' || bytes[index] == b'\'' {
            let quote = bytes[index];
            let start = index;
            index += 1;
            let mut escaped = false;
            while index < bytes.len() {
                let byte = bytes[index];
                index += 1;
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == quote {
                    break;
                }
            }
            if !matches!(name, "assert" | "debug_assert") {
                atoms.push(expected[start..index].to_owned());
            }
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"::") {
            index += 2;
            let start = index;
            while bytes
                .get(index)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            {
                index += 1;
            }
            let identifier = &expected[start..index];
            if !matches!(name, "assert" | "debug_assert")
                && identifier
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_uppercase)
                && !identifier.ends_with("V1")
                && !matches!(
                    identifier,
                    "Core" | "Filesystem" | "FsCas" | "CoreError" | "FsCasError"
                )
            {
                atoms.push(format!("::{identifier}"));
            }
            continue;
        }
        if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            index += 1;
            while bytes
                .get(index)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            {
                index += 1;
            }
            let identifier = &expected[start..index];
            if matches!(
                identifier,
                "true" | "false" | "None" | "Unavailable" | "NotApplicable"
            ) {
                atoms.push(identifier.to_owned());
            } else if !matches!(name, "assert" | "debug_assert")
                && identifier
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_uppercase)
                && !identifier.ends_with("V1")
                && !matches!(identifier, "Ok" | "Err" | "Some")
                && !matches!(
                    bytes.get(index..).and_then(|tail| {
                        tail.iter()
                            .position(|byte| !byte.is_ascii_whitespace())
                            .map(|offset| &tail[offset..])
                    }),
                    Some([b':', b':', ..] | [b'(', ..] | [b'{', ..])
                )
            {
                atoms.push(
                    constant_identifiers(identifier)
                        .into_iter()
                        .next()
                        .unwrap_or_else(|| identifier.to_owned()),
                );
            }
            continue;
        }
        index += 1;
    }
    match name {
        "assert_eq" | "debug_assert_eq" => atoms.push("==".to_owned()),
        "assert_ne" | "debug_assert_ne" => atoms.push("!=".to_owned()),
        _ => atoms.extend(
            predicate_operator_nodes(expected)
                .into_iter()
                .map(|operator| operator.trim_start_matches('#').to_owned()),
        ),
    }
    if matches!(name, "assert" | "debug_assert") {
        atoms.push("true".to_owned());
    }
    for (predicate, atom) in [
        (".is_ok()", "Ok"),
        (".is_err()", "Err"),
        (".is_some()", "Some"),
        (".is_none()", "None"),
        (".is_empty()", "0"),
    ] {
        if expected.contains(predicate) {
            atoms.push(atom.to_owned());
        }
    }
    atoms.sort();
    atoms.dedup();
    atoms
}

fn predicate_operator_nodes(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut operators = Vec::new();
    let mut index = 0_usize;
    while index < bytes.len() {
        let operator = ["==", "!=", "<=", ">="]
            .into_iter()
            .find(|operator| bytes.get(index..index + 2) == Some(operator.as_bytes()))
            .or_else(|| {
                matches!(bytes[index], b'<' | b'>')
                    .then(|| &source[index..index + 1])
                    .or_else(|| {
                        (bytes[index] == b'!' && bytes.get(index + 1) != Some(&b'=')).then_some("!")
                    })
            });
        if let Some(operator) = operator {
            operators.push(format!("#{operator}"));
            index += operator.len();
        } else {
            index += 1;
        }
    }
    if source.contains(".is_empty()") {
        operators.push("#==".to_owned());
    }
    operators.sort();
    operators.dedup();
    operators
}

fn semantic_identifiers(source: &str, endpoints_only: bool) -> Vec<String> {
    const IGNORED: [&str; 48] = [
        "assert",
        "debug_assert",
        "assert_eq",
        "assert_ne",
        "debug_assert_eq",
        "debug_assert_ne",
        "matches",
        "let",
        "self",
        "mut",
        "ref",
        "some",
        "none",
        "ok",
        "err",
        "unwrap",
        "unwrap_err",
        "is_ok",
        "is_err",
        "is_some",
        "is_none",
        "is_empty",
        "into",
        "from",
        "with",
        "within",
        "true",
        "false",
        "usize",
        "u64",
        "u32",
        "u16",
        "u8",
        "as",
        "get",
        "iter",
        "map",
        "sum",
        "copied",
        "downcast_ref",
        "std",
        "fs",
        "io",
        "time",
        "layerfs_storage",
        "path",
        "join",
        "metadata",
    ];
    let bytes = source.as_bytes();
    let mut selectors = Vec::new();
    let mut index = 0_usize;
    let mut string = false;
    let mut character = false;
    let mut escaped = false;
    while index < bytes.len() {
        if string || character {
            let byte = bytes[index];
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if (string && byte == b'"') || (character && byte == b'\'') {
                string = false;
                character = false;
            }
            index += 1;
            continue;
        }
        if bytes[index] == b'"' {
            string = true;
            index += 1;
            continue;
        }
        if bytes[index] == b'\'' && source_char_literal_start(bytes, index) {
            character = true;
            index += 1;
            continue;
        }
        if !bytes[index].is_ascii_alphabetic() && bytes[index] != b'_' {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            index += 1;
        }
        let identifier = &source[start..index];
        let member = bytes.get(start.wrapping_sub(1)) == Some(&b'.');
        let receiver =
            bytes.get(index) == Some(&b'.') || bytes.get(index..index + 2) == Some(b"::");
        if (endpoints_only && !receiver && !member)
            || identifier
                .strip_prefix("mut")
                .is_some_and(|tail| tail.as_bytes().first().is_some_and(u8::is_ascii_uppercase))
            || identifier
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_uppercase)
            || identifier.ends_with("V1")
            || IGNORED.contains(&identifier.to_ascii_lowercase().as_str())
        {
            continue;
        }
        selectors.push(identifier.to_owned());
    }
    selectors.sort();
    selectors.dedup();
    selectors
}

fn constant_identifiers(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut names = Vec::new();
    let mut index = 0_usize;
    while index < bytes.len() {
        if !bytes[index].is_ascii_uppercase() {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || *byte == b'_')
        {
            index += 1;
        }
        let name = &source[start..index];
        if name.bytes().any(|byte| byte == b'_') {
            names.push(name.to_owned());
        }
    }
    names.sort();
    names.dedup();
    names
}

fn semantic_selectors(source: &str) -> Vec<String> {
    let mut selectors = semantic_identifiers(source, true);
    selectors.extend(called_function_names(source));
    selectors.sort();
    selectors.dedup();
    selectors
}

fn assertion_value_nodes(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut values = Vec::new();
    let mut index = 0_usize;
    while index < bytes.len() {
        if bytes[index] == b'"'
            || (bytes[index] == b'\'' && source_char_literal_start(bytes, index))
        {
            let quote = bytes[index];
            let start = index;
            index += 1;
            let mut escaped = false;
            while index < bytes.len() {
                let byte = bytes[index];
                index += 1;
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == quote {
                    break;
                }
            }
            values.push(format!("#{}", &source[start..index]));
            continue;
        }
        if bytes[index].is_ascii_digit() {
            if bytes.get(index.wrapping_sub(1)) == Some(&b'.') {
                index += 1;
                continue;
            }
            let start = index;
            index += 1;
            while bytes
                .get(index)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'.'))
            {
                index += 1;
            }
            values.push(format!("#{}", &source[start..index]));
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"::") {
            index += 2;
            let start = index;
            while bytes
                .get(index)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            {
                index += 1;
            }
            let identifier = &source[start..index];
            if identifier
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_uppercase)
                && !identifier.ends_with("V1")
            {
                values.push(format!("@{identifier}"));
            }
            continue;
        }
        if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            index += 1;
            while bytes
                .get(index)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            {
                index += 1;
            }
            let identifier = &source[start..index];
            if matches!(
                identifier,
                "true" | "false" | "None" | "Some" | "Ok" | "Err"
            ) {
                values.push(format!("#{identifier}"));
            }
            continue;
        }
        index += 1;
    }
    for (predicate, value) in [
        (".is_ok()", "#Ok"),
        (".is_err()", "#Err"),
        (".is_some()", "#Some"),
        (".is_none()", "#None"),
        (".is_empty()", "#0"),
        (".err()", "#ResultErrProjection"),
    ] {
        if source.contains(predicate) {
            values.push(value.to_owned());
        }
    }
    values.extend(predicate_operator_nodes(source));
    values.sort();
    values.dedup();
    values
}

fn struct_field_colons(source: &str) -> std::collections::BTreeSet<usize> {
    let bytes = source.as_bytes();
    let mut fields = std::collections::BTreeSet::new();
    let mut delimiters = Vec::new();
    let mut string = false;
    let mut character = false;
    let mut escaped = false;
    for (index, byte) in bytes.iter().copied().enumerate() {
        if string || character {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if (string && byte == b'"') || (character && byte == b'\'') {
                string = false;
                character = false;
            }
            continue;
        }
        match byte {
            b'"' => string = true,
            b'\'' if source_char_literal_start(bytes, index) => character = true,
            b'(' | b'[' | b'{' => delimiters.push(byte),
            b')' | b']' | b'}' => {
                delimiters.pop();
            }
            b':' if bytes.get(index.wrapping_sub(1)) != Some(&b':')
                && bytes.get(index + 1) != Some(&b':')
                && delimiters.last() == Some(&b'{') =>
            {
                fields.insert(index);
            }
            _ => {}
        }
    }
    fields
}

fn function_tail_expression(body: &str) -> Option<&str> {
    let bytes = body.as_bytes();
    let mut start = 0_usize;
    let mut parentheses = 0_u32;
    let mut brackets = 0_u32;
    let mut braces = 0_u32;
    let mut string = false;
    let mut character = false;
    let mut escaped = false;
    for (index, byte) in bytes.iter().copied().enumerate() {
        if string || character {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if (string && byte == b'"') || (character && byte == b'\'') {
                string = false;
                character = false;
            }
            continue;
        }
        match byte {
            b'"' => string = true,
            b'\'' if source_char_literal_start(bytes, index) => character = true,
            b'(' => parentheses += 1,
            b')' => parentheses = parentheses.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            b';' if parentheses == 0 && brackets == 0 && braces == 0 => start = index + 1,
            _ => {}
        }
    }
    let tail = body[start..].trim();
    (!tail.is_empty()).then_some(tail)
}

fn assertion_expression_ends(bytes: &[u8], comma_terminates: bool) -> Vec<usize> {
    let mut matching_close = vec![None; bytes.len()];
    let mut parentheses = Vec::new();
    let mut brackets = Vec::new();
    let mut braces = Vec::new();
    for (index, byte) in bytes.iter().copied().enumerate() {
        let stack = match byte {
            b'(' | b')' => &mut parentheses,
            b'[' | b']' => &mut brackets,
            b'{' | b'}' => &mut braces,
            _ => continue,
        };
        if matches!(byte, b'(' | b'[' | b'{') {
            stack.push(index);
        } else if let Some(open) = stack.pop() {
            matching_close[open] = Some(index);
        }
    }

    let mut ends = vec![bytes.len(); bytes.len() + 1];
    for index in (0..bytes.len()).rev() {
        ends[index] = if bytes[index] == b';' || (comma_terminates && bytes[index] == b',') {
            index
        } else if matches!(bytes[index], b'(' | b'[' | b'{') {
            matching_close[index].map_or(bytes.len(), |close| ends[close + 1])
        } else {
            ends[index + 1]
        };
    }
    ends
}

fn assertion_bridge_graph(
    evidence: &str,
) -> std::collections::BTreeMap<String, std::collections::BTreeSet<String>> {
    use std::collections::{BTreeMap, BTreeSet};

    let profile_start = std::time::Instant::now();
    let source = evidence;
    let bytes = source.as_bytes();
    let field_colons = struct_field_colons(source);
    let statement_ends = assertion_expression_ends(bytes, false);
    let field_ends = assertion_expression_ends(bytes, true);
    let mut graph = BTreeMap::<String, BTreeSet<String>>::new();
    let mut left_start = 0_usize;
    let left_starts = bytes
        .iter()
        .copied()
        .enumerate()
        .map(|(index, byte)| {
            let start = left_start;
            if matches!(byte, b';' | b',' | b'{' | b'}') {
                left_start = index + 1;
            }
            start
        })
        .collect::<Vec<_>>();
    let mut right_targets = BTreeMap::<usize, (usize, BTreeSet<String>)>::new();
    let mut assignment_count = 0_usize;
    let mut assignment_right_bytes = 0_usize;
    let mut assignment_right_max = 0_usize;
    let mut assignment_parsed_bytes = 0_usize;
    let reference_analysis = std::env::var_os("PB08_CUSTODY_REFERENCE_ANALYSIS").is_some();
    for operator in (0..bytes.len()).rev() {
        let assignment = bytes[operator] == b'='
            && bytes.get(operator.wrapping_sub(1)) != Some(&b'=')
            && bytes.get(operator.wrapping_sub(1)) != Some(&b'!')
            && bytes.get(operator.wrapping_sub(1)) != Some(&b'<')
            && bytes.get(operator.wrapping_sub(1)) != Some(&b'>')
            && bytes.get(operator + 1) != Some(&b'=')
            && bytes.get(operator + 1) != Some(&b'>');
        let field = field_colons.contains(&operator);
        let arm = bytes[operator] == b'=' && bytes.get(operator + 1) == Some(&b'>');
        if !assignment && !field && !arm {
            continue;
        }
        let left_source = &source[left_starts[operator]..operator];
        let left = if arm {
            semantic_identifiers(left_source, false)
                .into_iter()
                .chain(assertion_value_nodes(left_source))
                .collect::<Vec<_>>()
        } else if left_source.contains("const ") || left_source.contains("static ") {
            constant_identifiers(left_source)
        } else if left_source.contains('(') || left_source.contains('[') {
            semantic_identifiers(left_source, false)
        } else {
            semantic_identifiers(left_source, false)
                .into_iter()
                .last()
                .into_iter()
                .collect()
        };
        if left.is_empty() {
            continue;
        }
        let right_end = if field {
            field_ends[operator + 1]
        } else {
            statement_ends[operator + 1]
        };
        let right = &source[operator + 1..right_end];
        assignment_count += 1;
        assignment_right_bytes += right.len();
        assignment_right_max = assignment_right_max.max(right.len());
        let uncached_targets;
        let targets = if reference_analysis {
            assignment_parsed_bytes += right.len();
            uncached_targets = semantic_identifiers(right, false)
                .into_iter()
                .chain(constant_identifiers(right))
                .chain(assertion_value_nodes(right))
                .collect::<BTreeSet<_>>();
            &uncached_targets
        } else {
            &right_targets
                .entry(right_end)
                .and_modify(|(cached_start, targets)| {
                    assert!(operator + 1 <= *cached_start);
                    let added = &source[operator + 1..*cached_start];
                    assignment_parsed_bytes += added.len();
                    targets.extend(semantic_identifiers(added, false));
                    targets.extend(constant_identifiers(added));
                    targets.extend(assertion_value_nodes(added));
                    *cached_start = operator + 1;
                })
                .or_insert_with(|| {
                    assignment_parsed_bytes += right.len();
                    (
                        operator + 1,
                        semantic_identifiers(right, false)
                            .into_iter()
                            .chain(constant_identifiers(right))
                            .chain(assertion_value_nodes(right))
                            .collect(),
                    )
                })
                .1
        };
        if arm {
            for target in targets {
                for source in left.iter().filter(|source| *source != target) {
                    graph
                        .entry(target.clone())
                        .or_default()
                        .insert(source.clone());
                }
            }
        } else {
            for left in &left {
                for target in targets.iter().filter(|target| *target != left) {
                    graph
                        .entry(left.clone())
                        .or_default()
                        .insert(target.clone());
                }
            }
        }
    }
    let assignments_done = std::time::Instant::now();
    for (match_start, _) in evidence.match_indices("match ") {
        let expression_start = match_start + "match ".len();
        let Some(body_open) = evidence[expression_start..]
            .find('{')
            .map(|offset| expression_start + offset)
        else {
            continue;
        };
        let Some(body_end) = brace_end(evidence, body_open) else {
            continue;
        };
        let scrutinee = semantic_identifiers(&evidence[expression_start..body_open], false);
        let body = &evidence[body_open + 1..body_end - 1];
        let mut arm_start = 0_usize;
        let mut parentheses = 0_u32;
        let mut brackets = 0_u32;
        let mut braces = 0_u32;
        let body_bytes = body.as_bytes();
        let mut cursor = 0_usize;
        while cursor + 1 < body_bytes.len() {
            match body_bytes[cursor] {
                b'(' => parentheses += 1,
                b')' => parentheses = parentheses.saturating_sub(1),
                b'[' => brackets += 1,
                b']' => brackets = brackets.saturating_sub(1),
                b'{' => braces += 1,
                b'}' => braces = braces.saturating_sub(1),
                b',' if parentheses == 0 && brackets == 0 && braces == 0 => {
                    arm_start = cursor + 1;
                }
                b'=' if body_bytes[cursor + 1] == b'>'
                    && parentheses == 0
                    && brackets == 0
                    && braces == 0 =>
                {
                    let pattern = body[arm_start..cursor].trim();
                    for wrapper in ["Ok(", "Err("] {
                        let Some(binding) = pattern
                            .strip_prefix(wrapper)
                            .and_then(|tail| tail.strip_suffix(')'))
                            .map(str::trim)
                            .filter(|binding| {
                                !binding.is_empty()
                                    && binding
                                        .bytes()
                                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                                    && binding.as_bytes().first().is_some_and(|byte| {
                                        byte.is_ascii_lowercase() || *byte == b'_'
                                    })
                            })
                        else {
                            continue;
                        };
                        for source in scrutinee.iter().filter(|source| source.as_str() != binding) {
                            graph
                                .entry(binding.to_owned())
                                .or_default()
                                .insert(source.clone());
                        }
                    }
                }
                _ => {}
            }
            cursor += 1;
        }
    }
    let matches_done = std::time::Instant::now();
    let index = semantic_source_index(evidence);
    for name in index.calls {
        let definitions = index
            .definitions
            .get(&name)
            .into_iter()
            .flatten()
            .flat_map(|definition| function_definitions(definition, &name))
            .collect::<Vec<_>>();
        if definitions.is_empty() {
            continue;
        }
        let tails = definitions
            .iter()
            .filter_map(|(_, body)| function_tail_expression(body))
            .collect::<Vec<_>>();
        let uniform_tail = (!tails.is_empty()).then(|| {
            tails
                .iter()
                .map(|expression| normalized_semantic_source(expression))
                .collect::<std::collections::BTreeSet<_>>()
        });
        if definitions.len() == 1
            || uniform_tail
                .as_ref()
                .is_some_and(|expressions| expressions.len() == 1)
        {
            let expression = if definitions.len() == 1 {
                function_tail_expression(definitions[0].1)
            } else {
                tails.first().copied()
            };
            let Some(expression) = expression else {
                continue;
            };
            for target in semantic_identifiers(expression, false)
                .into_iter()
                .chain(constant_identifiers(expression))
                .chain(assertion_value_nodes(expression))
                .filter(|target| target != &name)
            {
                graph
                    .entry(name.clone())
                    .or_default()
                    .insert(target.clone());
            }
        }
    }
    let definitions_done = std::time::Instant::now();
    for (if_start, _) in evidence.match_indices("if ") {
        let condition_start = if_start + 3;
        let Some(body_open) = evidence[condition_start..]
            .find('{')
            .map(|offset| condition_start + offset)
        else {
            continue;
        };
        let Some(body_end) = brace_end(evidence, body_open) else {
            continue;
        };
        let condition_nodes = semantic_identifiers(&evidence[condition_start..body_open], false)
            .into_iter()
            .chain(assertion_value_nodes(&evidence[condition_start..body_open]))
            .collect::<Vec<_>>();
        let body = &evidence[body_open + 1..body_end - 1];
        for operator in body.match_indices('=').map(|(index, _)| index) {
            let left_start = body[..operator]
                .rfind([';', ',', '{', '}'])
                .map_or(0, |position| position + 1);
            let Some(target) = semantic_identifiers(&body[left_start..operator], false)
                .into_iter()
                .last()
            else {
                continue;
            };
            for condition in condition_nodes
                .iter()
                .filter(|condition| **condition != target)
            {
                graph
                    .entry(target.clone())
                    .or_default()
                    .insert(condition.clone());
            }
        }
    }
    if std::env::var_os("PB08_CUSTODY_BRIDGE_PROFILE").is_some() {
        let done = std::time::Instant::now();
        eprintln!(
            "PB08 bridge: bytes={} assignments={} right_bytes={} parsed_bytes={} right_max={} assignment_time={:.3}s matches={:.3}s definitions={:.3}s conditions={:.3}s nodes={}",
            evidence.len(),
            assignment_count,
            assignment_right_bytes,
            assignment_parsed_bytes,
            assignment_right_max,
            assignments_done.duration_since(profile_start).as_secs_f64(),
            matches_done.duration_since(assignments_done).as_secs_f64(),
            definitions_done.duration_since(matches_done).as_secs_f64(),
            done.duration_since(definitions_done).as_secs_f64(),
            graph.len()
        );
    }
    graph
}

fn assertion_bridge_graphs(
    evidence: &[&str],
    parallel: bool,
) -> Vec<std::collections::BTreeMap<String, std::collections::BTreeSet<String>>> {
    let workers = if parallel {
        std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1)
            .min(4)
            .min(evidence.len())
    } else {
        1.min(evidence.len())
    };
    if workers <= 1 {
        return evidence
            .iter()
            .map(|evidence| assertion_bridge_graph(evidence))
            .collect();
    }

    let mut graphs = std::thread::scope(|scope| {
        (0..workers)
            .map(|worker| {
                scope.spawn(move || {
                    evidence
                        .iter()
                        .enumerate()
                        .skip(worker)
                        .step_by(workers)
                        .map(|(ordinal, evidence)| (ordinal, assertion_bridge_graph(evidence)))
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .flat_map(|worker| worker.join().expect("PB-08 bridge worker panicked"))
            .collect::<Vec<_>>()
    });
    graphs.sort_by_key(|(ordinal, _)| *ordinal);
    graphs.into_iter().map(|(_, graph)| graph).collect()
}

fn bridge_reaches(
    graph: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    cache: &mut std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    start: &str,
    targets: &std::collections::BTreeSet<String>,
) -> bool {
    use std::collections::{BTreeSet, VecDeque};

    let reachable = cache.entry(start.to_owned()).or_insert_with(|| {
        let mut pending = VecDeque::from([start.to_owned()]);
        let mut visited = BTreeSet::new();
        while let Some(node) = pending.pop_front() {
            if !visited.insert(node.clone()) {
                continue;
            }
            if let Some(neighbors) = graph.get(&node) {
                pending.extend(
                    neighbors
                        .iter()
                        .filter(|neighbor| !visited.contains(*neighbor))
                        .cloned(),
                );
            }
        }
        visited
    });
    !reachable.is_disjoint(targets)
}

fn bridged_nodes(
    graph: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    cache: &mut std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    starts: impl IntoIterator<Item = String>,
) -> std::collections::BTreeSet<String> {
    let mut nodes = std::collections::BTreeSet::new();
    for start in starts {
        let _ = bridge_reaches(graph, cache, &start, &std::collections::BTreeSet::new());
        nodes.extend(cache[&start].iter().cloned());
    }
    nodes
}

fn assertion_contract_gaps(
    historical: &[String],
    final_assertions: &[String],
    historical_evidence: &str,
    final_evidence: &str,
) -> Vec<String> {
    let historical_graph = assertion_bridge_graph(historical_evidence);
    let final_graph = assertion_bridge_graph(final_evidence);
    assertion_contract_gaps_with_graphs(
        historical,
        final_assertions,
        &historical_graph,
        &final_graph,
    )
}

fn assertion_contract_gaps_with_graphs(
    historical: &[String],
    final_assertions: &[String],
    historical_graph: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    final_graph: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
) -> Vec<String> {
    use std::collections::{BTreeMap, BTreeSet};

    let mut historical_reachability = BTreeMap::new();
    let mut final_reachability = BTreeMap::new();
    let final_predicates = final_assertions
        .iter()
        .map(|candidate| {
            let arguments = assertion_arguments(candidate);
            let candidate_subject = arguments.first().copied().unwrap_or(candidate);
            let candidate_expected = assertion_expected_expression(candidate);
            let candidate_selectors = semantic_selectors(candidate_subject)
                .into_iter()
                .chain(semantic_identifiers(candidate_subject, false))
                .collect::<BTreeSet<_>>();
            let candidate_subjects = bridged_nodes(
                &final_graph,
                &mut final_reachability,
                candidate_selectors.iter().cloned(),
            );
            let mut candidate_starts = semantic_selectors(candidate_expected)
                .into_iter()
                .chain(semantic_identifiers(candidate_expected, false))
                .chain(constant_identifiers(candidate_expected))
                .chain(assertion_value_nodes(candidate_expected))
                .collect::<BTreeSet<_>>();
            for atom in assertion_expected_atoms(candidate) {
                candidate_starts.extend(assertion_atom_nodes(&atom));
            }
            let mut candidate_expected_nodes =
                bridged_nodes(&final_graph, &mut final_reachability, candidate_starts);
            let direct_operators = predicate_operator_nodes(candidate_expected)
                .into_iter()
                .collect::<BTreeSet<_>>();
            let mut candidate_atoms = assertion_expected_atoms(candidate)
                .into_iter()
                .collect::<BTreeSet<_>>();
            candidate_atoms.extend(candidate_expected_nodes.iter().filter_map(|node| {
                node.strip_prefix('#').map(str::to_owned).or_else(|| {
                    constant_identifiers(node)
                        .into_iter()
                        .any(|constant| constant == *node)
                        .then(|| node.clone())
                })
            }));
            if candidate_expected_nodes.contains("#None")
                && candidate_subjects.contains("#ResultErrProjection")
            {
                candidate_atoms.extend(["Ok".to_owned(), "true".to_owned()]);
                candidate_expected_nodes.insert("#Ok".to_owned());
            }
            if direct_operators.contains("#!") {
                let mut complements = Vec::new();
                for (positive, negative) in [
                    ("#Some", "None"),
                    ("#None", "Some"),
                    ("#Ok", "Err"),
                    ("#Err", "Ok"),
                    ("#true", "false"),
                    ("#false", "true"),
                ] {
                    if candidate_expected_nodes.contains(positive) {
                        candidate_atoms.insert(negative.to_owned());
                        complements.push(format!("#{negative}"));
                    }
                }
                if !complements.is_empty() {
                    candidate_atoms.insert("==".to_owned());
                }
                candidate_expected_nodes.extend(complements);
            }
            if direct_operators.is_empty() && assertion_affirms_predicate(candidate) {
                candidate_expected_nodes.extend(candidate_subjects.iter().cloned());
                candidate_atoms.extend(
                    candidate_expected_nodes
                        .iter()
                        .chain(&candidate_subjects)
                        .filter_map(|node| {
                            node.strip_prefix('#').map(str::to_owned).or_else(|| {
                                node.strip_prefix('@').map(|value| format!("::{value}"))
                            })
                        }),
                );
            }
            (
                candidate_subjects,
                candidate_expected_nodes,
                candidate_atoms,
                direct_operators,
                assertion_is_tautology(candidate),
            )
        })
        .collect::<Vec<_>>();
    let mut compatibility = Vec::with_capacity(historical.len());
    let mut gaps = Vec::with_capacity(historical.len());
    for historical_assertion in historical {
        let mut candidates = final_assertions
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| (candidate == historical_assertion).then_some(index))
            .collect::<Vec<_>>();
        let atoms = assertion_expected_atoms(historical_assertion);
        let atom_set = atoms.iter().cloned().collect::<BTreeSet<_>>();
        let historical_arguments = assertion_arguments(historical_assertion);
        let selectors = historical_arguments
            .iter()
            .take(1)
            .flat_map(|argument| {
                semantic_selectors(argument)
                    .into_iter()
                    .chain(semantic_identifiers(argument, false))
            })
            .collect::<BTreeSet<_>>();
        let historical_subjects = bridged_nodes(
            &historical_graph,
            &mut historical_reachability,
            selectors.iter().cloned(),
        );
        let historical_expected = assertion_expected_expression(historical_assertion);
        let historical_operators = predicate_operator_nodes(historical_expected)
            .into_iter()
            .collect::<BTreeSet<_>>();
        let historical_expected_roots = semantic_selectors(historical_expected)
            .into_iter()
            .chain(semantic_identifiers(historical_expected, false))
            .chain(constant_identifiers(historical_expected))
            .chain(assertion_value_nodes(historical_expected))
            .collect::<BTreeSet<_>>();
        let historical_expected_nodes = bridged_nodes(
            &historical_graph,
            &mut historical_reachability,
            historical_expected_roots,
        );
        let historical_atom_nodes = atoms
            .iter()
            .map(|atom| {
                if constant_identifiers(atom).iter().any(|name| name == atom) {
                    bridged_nodes(
                        &historical_graph,
                        &mut historical_reachability,
                        [atom.clone()],
                    )
                } else {
                    BTreeSet::new()
                }
            })
            .collect::<Vec<_>>();
        candidates.extend(final_predicates.iter().enumerate().filter_map(
            |(
                index,
                (
                    candidate_subjects,
                    candidate_expected_nodes,
                    candidate_atoms,
                    candidate_operators,
                    tautology,
                ),
            )| {
                ((!*tautology || assertion_is_tautology(historical_assertion))
                    && !historical_subjects.is_empty()
                    && !candidate_subjects.is_disjoint(&historical_subjects)
                    && atoms
                        .iter()
                        .zip(&historical_atom_nodes)
                        .all(|(atom, nodes)| {
                            candidate_atoms.contains(atom)
                                || !candidate_expected_nodes.is_disjoint(nodes)
                        })
                    && (candidate_operators.is_empty()
                        || *candidate_operators == historical_operators
                        || (historical_operators.is_empty()
                            && atom_set.contains("==")
                            && *candidate_operators == BTreeSet::from(["#!".to_owned()])
                            && ["None", "Some", "Ok", "Err", "true", "false"]
                                .iter()
                                .any(|atom| atom_set.contains(*atom))))
                    && !historical_expected_nodes.is_empty()
                    && !candidate_expected_nodes.is_disjoint(&historical_expected_nodes))
                .then_some(index)
            },
        ));
        candidates.sort_unstable();
        candidates.dedup();
        let candidate_debug = candidates.clone();
        compatibility.push(candidates);
        gaps.push(format!(
            "{historical_assertion:?} has no attributable final assertion for selectors {selectors:?}, atoms {atoms:?}, and candidates {candidate_debug:?}"
        ));
    }

    fn assign(
        historical: usize,
        compatibility: &[Vec<usize>],
        visited: &mut [bool],
        owners: &mut [Option<usize>],
    ) -> bool {
        for &candidate in &compatibility[historical] {
            if visited[candidate] {
                continue;
            }
            visited[candidate] = true;
            let previous = owners[candidate];
            if previous.is_none()
                || assign(
                    previous.expect("checked above"),
                    compatibility,
                    visited,
                    owners,
                )
            {
                owners[candidate] = Some(historical);
                return true;
            }
        }
        false
    }

    let mut owners = vec![None; final_assertions.len()];
    let mut unmatched = Vec::new();
    for historical in 0..historical.len() {
        let mut visited = vec![false; final_assertions.len()];
        if !assign(historical, &compatibility, &mut visited, &mut owners) {
            unmatched.push(gaps[historical].clone());
        }
    }
    unmatched
}

fn assertion_contract_gap(
    historical: &[String],
    final_assertions: &[String],
    historical_evidence: &str,
    final_evidence: &str,
) -> Option<String> {
    assertion_contract_gaps(
        historical,
        final_assertions,
        historical_evidence,
        final_evidence,
    )
    .into_iter()
    .next()
}

fn assertion_expected_expression(assertion: &str) -> &str {
    let arguments = assertion_arguments(assertion);
    let name = &assertion[..assertion.find('!').expect("assertion macro bang")];
    match name {
        "assert_eq" | "assert_ne" | "debug_assert_eq" | "debug_assert_ne" | "matches" => {
            arguments.get(1).copied().unwrap_or_default()
        }
        _ => arguments.first().copied().unwrap_or_default(),
    }
}

fn assertion_is_tautology(assertion: &str) -> bool {
    let name = &assertion[..assertion.find('!').expect("assertion macro bang")];
    if !matches!(name, "assert_eq" | "debug_assert_eq") {
        return false;
    }
    let arguments = assertion_arguments(assertion);
    arguments.len() >= 2
        && normalized_semantic_source(arguments[0]) == normalized_semantic_source(arguments[1])
}

fn assertion_affirms_predicate(assertion: &str) -> bool {
    let name = &assertion[..assertion.find('!').expect("assertion macro bang")];
    let arguments = assertion_arguments(assertion);
    match name {
        "assert" | "debug_assert" => !arguments.is_empty(),
        "assert_eq" | "debug_assert_eq" => arguments
            .get(1)
            .is_some_and(|expected| normalized_semantic_source(expected) == "true"),
        _ => false,
    }
}

#[test]
fn pb08_assertion_contract_matcher_is_mutation_sensitive() {
    let imports = "use layerfs_storage::qualification::cas::semantic::{\
                   carrier_cleanup_failure_v1};\
                   use layerfs_storage::qualification::lifecycle::semantic::{\
                   carrier_cleanup_failure_v1 as lifecycle_carrier_cleanup_failure_v1};";
    assert!(qualification_function_is_imported(
        imports,
        "cas",
        "carrier_cleanup_failure_v1"
    ));
    assert!(!qualification_function_is_imported(
        imports,
        "lifecycle",
        "carrier_cleanup_failure_v1"
    ));
    let admission_source = include_str!("cas_admission.rs");
    assert!(qualification_function_is_imported(
        admission_source,
        "cas",
        "valid_locator_binding_mismatches_v1"
    ));
    assert!(qualification_function_is_imported(
        include_str!("operation_read.rs"),
        "object",
        "decode_v1"
    ));
    assert!(qualification_function_is_imported(
        "use layerfs_storage::qualification::resources::{observe_memory_profile_v1};",
        "resources",
        "observe_memory_profile_v1"
    ));
    let cas_source = include_str!("../src/cas/mod.rs").to_owned();
    let mut cas_names = called_function_names(&cas_source);
    cas_names.insert("valid_locator_binding_mismatches_v1".to_owned());
    let cas_definitions =
        closed_function_definitions_for_names(std::slice::from_ref(&cas_source), &cas_names);
    assert!(reachable_function_definitions(
        ["valid_locator_binding_mismatches_v1".to_owned()],
        &cas_definitions,
    )
    .contains_key("build_publication_pack_raw"));

    fn gap(historical: &[&str], final_assertions: &[&str], bridge: &str) -> bool {
        assertion_contract_gap(
            &historical
                .iter()
                .map(|value| (*value).to_owned())
                .collect::<Vec<_>>(),
            &final_assertions
                .iter()
                .map(|value| (*value).to_owned())
                .collect::<Vec<_>>(),
            bridge,
            bridge,
        )
        .is_some()
    }

    assert!(!gap(
        &["assert_eq!(actual,expected)"],
        &["assert_eq!(observation,expected)"],
        "let actual=source;let observation=source;",
    ));
    for changed in [
        "assert_eq!(observation,observation)",
        "assert_eq!(observation,other)",
        "assert_ne!(observation,expected)",
        "assert_eq!(observation,999)",
        "assert_eq!(observation,\"changed\")",
        "assert_eq!(observation,Err(CoreError::Deadline))",
    ] {
        assert!(gap(
            &["assert_eq!(actual,expected)"],
            &[changed],
            "let actual=source;let observation=source;",
        ));
    }
    assert!(gap(
        &["assert_eq!(actual,Err(CoreError::Cancelled))"],
        &["assert_eq!(observation,Err(CoreError::Deadline))"],
        "let actual=source;let observation=source;",
    ));
    let constant_bridge =
        "const SEEDED_BYTES_READ:u64=71;let actual=source;let observation=source;";
    assert!(!gap(
        &["assert_eq!(actual,SEEDED_BYTES_READ)"],
        &["assert_eq!(observation,71)"],
        constant_bridge,
    ));
    assert!(gap(
        &["assert_eq!(actual,SEEDED_BYTES_READ)"],
        &["assert_eq!(observation,72)"],
        constant_bridge,
    ));
    let repeated_constant_accessors = "const BASE_LEDGER_BYTES:u64=8;\
                                       const OPERATION_SLOT_BYTES:u64=4;\
                                       fn base_budget_bytes()->u64{BASE_LEDGER_BYTES}\
                                       fn base_budget_bytes()->u64{BASE_LEDGER_BYTES}\
                                       fn operation_slot_bytes()->u64{OPERATION_SLOT_BYTES}\
                                       fn operation_slot_bytes()->u64{OPERATION_SLOT_BYTES}\
                                       let _=base_budget_bytes()+operation_slot_bytes();\
                                       let observation=Observation{\
                                           memory_high_water:counters.memory_high_water\
                                       };\
                                       fn memory_high_water(&self)->u64{self.memory_high_water}";
    assert!(!gap(
        &["assert_eq!(counters.memory_high_water,BASE_LEDGER_BYTES+OPERATION_SLOT_BYTES)"],
        &["assert_eq!(observation.memory_high_water(),base_budget_bytes()+operation_slot_bytes())"],
        repeated_constant_accessors,
    ));
    assert!(gap(
        &["assert_eq!(counters.memory_high_water,BASE_LEDGER_BYTES+OPERATION_SLOT_BYTES)"],
        &["assert_eq!(observation.memory_high_water(),base_budget_bytes()+base_budget_bytes())"],
        repeated_constant_accessors,
    ));
    let counter_field_bridge = "let observation=Observation{\
                                bytes_written:counters.fscas_bytes_written\
                                };\
                                fn bytes_written(&self)->u64{self.bytes_written}";
    assert!(!gap(
        &["assert_eq!(counters.fscas_bytes_written,0)"],
        &["assert_eq!(observation.bytes_written(),0)"],
        counter_field_bridge,
    ));
    assert!(gap(
        &["assert_eq!(counters.fscas_bytes_written,0)"],
        &["assert_eq!(observation.bytes_written(),1)"],
        counter_field_bridge,
    ));
    assert!(!gap(
        &["assert_eq!(counters.fscas_bytes_written,0)"],
        &["assert_eq!(observation.bytes_written(),0)"],
        &cas_source,
    ));
    let result_error_bridge = "let observation=Observation{\
                               error:result.as_ref().err().copied()};\
                               fn error(&self)->Option<CoreError>{self.error}";
    assert!(!gap(
        &["assert!(result.is_ok())"],
        &["assert_eq!(observation.error(),None)"],
        result_error_bridge,
    ));
    assert!(gap(
        &["assert!(result.is_ok())"],
        &["assert_eq!(observation.error(),Some(CoreError::Cancelled))"],
        result_error_bridge,
    ));
    assert!(gap(
        &["assert!(lower<=actual&&actual<=upper)"],
        &["assert!(lower<observation&&observation<=upper)"],
        "let actual=source;let observation=source;",
    ));
    assert!(gap(
        &["assert!(!actual)"],
        &["assert!(observation)"],
        "let actual=source;let observation=source;",
    ));
    let option_bridge = "let sink_completed=sink.completed.is_some();\
                         let observation=Observation{sink_completed};\
                         fn sink_completed(&self)->bool{self.sink_completed}";
    assert!(!gap(
        &["assert_eq!(sink.completed,None)"],
        &["assert!(!observation.sink_completed())"],
        option_bridge,
    ));
    assert!(gap(
        &["assert_eq!(sink.completed,None)"],
        &["assert!(observation.sink_completed())"],
        option_bridge,
    ));
    let update_terminal_bridge = "let sink_completed=sink.completed.is_some();\
                                  let output_aborted=output.aborted;\
                                  let observation=Observation{sink_completed,output_aborted};\
                                  fn sink_completed(&self)->bool{self.sink_completed}\
                                  fn output_aborted(&self)->bool{self.output_aborted}";
    assert!(!gap(
        &[
            "assert_eq!(result.sink.completed,None)",
            "assert!(result.output.aborted)",
        ],
        &[
            "assert!(!observation.sink_completed())",
            "assert!(observation.output_aborted())",
        ],
        update_terminal_bridge,
    ));
    let exact_helper_bridge = "let actual=source;let observation=source;\
                               const BASE_BYTES:u64=1;const WINDOW_BYTES:u64=2;\
                               fn exact(bytes:u64)->u64{BASE_BYTES+WINDOW_BYTES*2+bytes}\
                               let _=exact(bytes);";
    assert!(!gap(
        &["assert_eq!(actual,BASE_BYTES+(WINDOW_BYTES as u64*2)+bytes)"],
        &["assert_eq!(observation,exact(bytes))"],
        exact_helper_bridge,
    ));
    assert!(gap(
        &["assert_eq!(actual,BASE_BYTES+(WINDOW_BYTES as u64*3)+bytes)"],
        &["assert_eq!(observation,exact(bytes))"],
        exact_helper_bridge,
    ));
    assert!(!gap(
        &["assert!(sink.committed.is_empty())"],
        &["assert!(observation.sink_committed_len()==0)"],
        "let sink_committed_len=sink.committed.len();",
    ));
    assert!(gap(
        &["assert_eq!(first,1)", "assert_eq!(second,1)"],
        &["assert_eq!(observation,1)"],
        "let first=source;let second=source;let observation=source;",
    ));
    assert!(!gap(
        &["assert_eq!(first,1)", "assert_eq!(second,1)"],
        &["assert_eq!(shared,1)", "assert_eq!(alternate,1)"],
        "let shared=first;let shared=second;let alternate=first;",
    ));
    assert!(gap(
        &["assert_eq!(actual,Err(Terminal{first:Cancelled,dominant:CleanupFailed}))"],
        &[
            "assert_eq!(observation,Err(Terminal{first:Cancelled}))",
            "assert_eq!(observation,Err(Terminal{dominant:CleanupFailed}))",
        ],
        "let actual=source;let observation=source;",
    ));
    assert!(!gap(
        &["assert!(cas.fixed_handle_charge()<=BASE)"],
        &["assert!(observation.fixed_handles_within_budget())"],
        "let fixed_handles_within_budget=cas.fixed_handle_charge()<=BASE;\
         let observation=Observation{fixed_handles_within_budget};\
         fn fixed_handles_within_budget(&self)->bool{self.fixed_handles_within_budget}",
    ));
    let installed_bridge = "let installed=admission.outcome()==Outcome::Installed;\
                            let observation=Observation{installed};\
                            fn installed(&self)->bool{self.installed}";
    assert!(!gap(
        &["assert_eq!(admission.outcome(),Outcome::Installed)"],
        &["assert_eq!(observation.installed(),true)"],
        installed_bridge,
    ));
    assert!(gap(
        &["assert_eq!(admission.outcome(),Outcome::Installed)"],
        &["assert_eq!(observation.installed(),false)"],
        installed_bridge,
    ));
    assert!(!gap(
        &["assert!(visitor.visible.is_empty())"],
        &["assert!(observation.edge_kinds().is_empty())"],
        "let observation=Observation{edge_kinds:visitor.visible};\
         fn edge_kinds(&self)->&[EdgeKind]{&self.edge_kinds}",
    ));
    let edge_kind_bridge = "let observation=Observation{edge_kinds:visitor.visible};\
                            fn edge_kinds(&self)->&[EdgeKindV1]{&self.edge_kinds}";
    assert!(!gap(
        &["matches!(visitor.visible[0],StrongEdgeV1::Tree(_))"],
        &["assert_eq!(observation.edge_kinds()[0],EdgeKindV1::Tree)"],
        edge_kind_bridge,
    ));
    assert!(gap(
        &["matches!(visitor.visible[0],StrongEdgeV1::Tree(_))"],
        &["assert_eq!(observation.edge_kinds()[0],EdgeKindV1::File)"],
        edge_kind_bridge,
    ));
    let object_adapter = include_str!("../src/object/mod.rs");
    assert!(!gap(
        &["matches!(visitor.visible[0],StrongEdgeV1::Tree(_))"],
        &["assert_eq!(observation.edge_kinds()[0],EdgeKindV1::Tree)"],
        object_adapter,
    ));
    assert!(gap(
        &["matches!(visitor.visible[0],StrongEdgeV1::Tree(_))"],
        &["assert_eq!(observation.edge_kinds()[0],EdgeKindV1::File)"],
        object_adapter,
    ));
    let result_bridge = "let result=decode();\
                         let observation=match result{\
                           Ok(value)=>Observation{error:None,value:Some(value)},\
                           Err(error)=>Observation{error:Some(error),value:None}};\
                         fn error(&self)->Option<CoreError>{self.error}";
    assert!(!gap(
        &["assert_eq!(decode(),Err(CoreError::NonCanonicalOrder))"],
        &["assert_eq!(observation.error(),Some(CoreError::NonCanonicalOrder))"],
        result_bridge,
    ));
    assert!(gap(
        &["assert_eq!(decode(),Err(CoreError::NonCanonicalOrder))"],
        &["assert_eq!(observation.error(),Some(CoreError::Deadline))"],
        result_bridge,
    ));
    for unchanged in [
        "assert!(bytes.len()>65_536)",
        "assert!(result.is_ok())",
        "assert!(result.is_err())",
        "matches!(result,Err(CoreError::Cancelled))",
        "assert_eq!(used,requested-reserved)",
    ] {
        assert!(!gap(&[unchanged], &[unchanged], ""));
    }

    let adapter = "fn outer(actual:u64){inner(actual)}\
                   fn inner(value:u64){assert_eq!(value,7)}";
    let mut candidates =
        std::collections::BTreeMap::from([("outer".to_owned(), vec![adapter.to_owned()])]);
    for (name, locations) in semantic_function_definitions(&[adapter.to_owned()]) {
        candidates.entry(name).or_default().extend(locations);
    }
    let nested = instantiated_helper_assertions(
        "outer(observation)",
        &unique_function_definitions(candidates),
    );
    assert_eq!(nested.len(), 1);
    assert!(normalized_semantic_source(&nested[0]).contains("observation"));
    assert!(normalized_semantic_source(&nested[0]).contains('7'));
    let substitutions = std::collections::BTreeMap::from([("cases".to_owned(), "16")]);
    assert_eq!(
        substitute_parameters(
            "assert_eq!(observation.cases(),cases);Observation{cases:cases}",
            &substitutions,
        ),
        "assert_eq!(observation.cases(),(16));Observation{cases:(16)}"
    );
    let duplicated = instantiated_helper_assertions(
        "outer(observation);outer(observation)",
        &unique_function_definitions(std::collections::BTreeMap::from([
            ("outer".to_owned(), vec![adapter.to_owned()]),
            ("inner".to_owned(), vec![adapter.to_owned()]),
        ])),
    );
    assert_eq!(duplicated, nested);
    let distinct = instantiated_helper_assertions(
        "outer(first);outer(second)",
        &unique_function_definitions(std::collections::BTreeMap::from([
            ("outer".to_owned(), vec![adapter.to_owned()]),
            ("inner".to_owned(), vec![adapter.to_owned()]),
        ])),
    );
    assert_eq!(distinct.len(), 2);
    assert!(normalized_semantic_source(&distinct[0]).contains("first"));
    assert!(normalized_semantic_source(&distinct[1]).contains("second"));

    let owner = "use layerfs_storage::qualification::cas::semantic::{scenario};\
                 fn wait(){assert!(false,\"unreachable local helper\")}\
                 #[test]fn row(){scenario()}";
    let cas = std::collections::BTreeMap::from([
        (
            "scenario".to_owned(),
            vec!["fn scenario(){wait()}".to_owned()],
        ),
        (
            "wait".to_owned(),
            vec!["fn wait(){assert!(*released,\"timed out\")}".to_owned()],
        ),
    ]);
    let lifecycle = std::collections::BTreeMap::from([
        ("other".to_owned(), vec!["fn other(){wait()}".to_owned()]),
        (
            "wait".to_owned(),
            vec!["fn wait(){assert!(false,\"wrong family\")}".to_owned()],
        ),
    ]);
    let families = [("cas", cas), ("lifecycle", lifecycle)];
    let local = scoped_function_definitions(
        owner,
        owner.find("#[test]").unwrap(),
        &std::collections::BTreeMap::new(),
    );
    let definitions = row_scoped_function_definitions(
        owner,
        "scenario()",
        &local,
        &families,
        &std::collections::BTreeMap::new(),
    );
    assert_eq!(
        instantiated_helper_assertions("scenario()", &definitions),
        ["assert!(*released,\"timed out\")"]
    );
}

fn assertion_atom_nodes(atom: &str) -> Vec<String> {
    if let Some(variant) = atom.strip_prefix("::") {
        return vec![format!("@{variant}")];
    }
    match atom {
        "Ok" => vec!["#Ok".to_owned()],
        "Err" => vec!["#Err".to_owned()],
        "Some" => vec!["#Some".to_owned()],
        "None" => vec!["#None".to_owned()],
        "Empty" => vec!["#0".to_owned()],
        _ => vec![format!("#{atom}")],
    }
}

fn called_function_names(source: &str) -> std::collections::BTreeSet<String> {
    called_function_names_from_tokens(source, &source_identifier_tokens(source))
}

fn called_function_names_from_tokens(
    source: &str,
    tokens: &[(usize, &str)],
) -> std::collections::BTreeSet<String> {
    const LANGUAGE_FORMS: [&str; 9] = [
        "if", "while", "for", "match", "loop", "Some", "Ok", "Err", "return",
    ];
    let mut names = std::collections::BTreeSet::new();
    for &(start, name) in tokens {
        let end = start + name.len();
        let open = skip_source_trivia(source, end);
        let function_item = [
            ".map(",
            ".map_err(",
            ".is_some_and(",
            ".is_ok_and(",
            ".sort_by(",
            ".sort_by_key(",
        ]
        .iter()
        .any(|prefix| source[..start].trim_end().ends_with(prefix));
        let declaration = source[..start].trim_end().ends_with("fn");
        if (source.as_bytes().get(open) == Some(&b'(') || function_item)
            && !declaration
            && !LANGUAGE_FORMS.contains(&name)
        {
            names.insert(name.to_owned());
        }
    }
    names
}

fn qualification_function_is_imported(source: &str, family: &str, name: &str) -> bool {
    let source = normalized_semantic_source(source);
    for prefix in [
        format!("layerfs_storage::qualification::{family}::semantic::"),
        format!("layerfs_storage::qualification::{family}::"),
        format!("layerfs_storage::{family}::semantic::"),
    ] {
        let mut remainder = source.as_str();
        while let Some(start) = remainder.find(&prefix) {
            remainder = &remainder[start + prefix.len()..];
            if let Some(imports) = remainder
                .strip_prefix('{')
                .and_then(|tail| tail.split_once('}').map(|(imports, _)| imports))
            {
                if imports.split(',').any(|import| import == name) {
                    return true;
                }
            } else {
                let end = remainder
                    .find(|character: char| {
                        !(character.is_ascii_alphanumeric() || character == '_')
                    })
                    .unwrap_or(remainder.len());
                if &remainder[..end] == name {
                    return true;
                }
            }
            remainder = &remainder[1..];
        }
    }
    false
}

fn function_definitions<'a>(source: &'a str, name: &str) -> Vec<(&'a str, &'a str)> {
    let signature = format!("fn {name}");
    source
        .match_indices(&signature)
        .filter_map(|(start, _)| {
            let before = source.as_bytes().get(start.wrapping_sub(1)).copied();
            let after = source.as_bytes().get(start + signature.len()).copied();
            let bounded_before =
                before.is_none_or(|byte| !(byte.is_ascii_alphanumeric() || byte == b'_'));
            let bounded_after =
                after.is_none_or(|byte| !(byte.is_ascii_alphanumeric() || byte == b'_'));
            if !bounded_before || !bounded_after {
                return None;
            }
            let tail = &source[start..];
            let open = tail.find('{')?;
            let mut brackets = 0_u32;
            for byte in tail[..open].bytes() {
                match byte {
                    b'[' => brackets += 1,
                    b']' => brackets = brackets.saturating_sub(1),
                    b';' if brackets == 0 => return None,
                    _ => {}
                }
            }
            Some((&tail[..open], function_body(tail, name)))
        })
        .collect()
}

fn semantic_function_definitions(
    corpus: &[String],
) -> std::collections::BTreeMap<String, Vec<String>> {
    if std::env::var_os("PB08_CUSTODY_REFERENCE_ANALYSIS").is_some() {
        let names = called_function_names(
            &corpus
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join("\n"),
        );
        return semantic_function_definitions_for_names_reference(corpus, &names);
    }
    let indexes = corpus
        .iter()
        .map(|source| semantic_source_index(source))
        .collect::<Vec<_>>();
    let names = indexes
        .iter()
        .flat_map(|index| index.calls.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>();
    semantic_function_definitions_from_indexes(&indexes, &names)
}

fn semantic_function_definitions_for_names_reference(
    corpus: &[String],
    names: &std::collections::BTreeSet<String>,
) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut definitions = std::collections::BTreeMap::<String, Vec<String>>::new();
    for name in names {
        for source in corpus {
            for (signature, body) in function_definitions(source, name) {
                let mut constants = String::new();
                let mut pending = constant_identifiers(body);
                let mut included = std::collections::BTreeSet::new();
                while let Some(constant) = pending.pop() {
                    if !included.insert(constant.clone()) {
                        continue;
                    }
                    let mut declarations = Vec::new();
                    for needle in [format!("const {constant}"), format!("static {constant}")] {
                        declarations.extend(source.match_indices(&needle).map(|(start, _)| start));
                    }
                    if declarations.len() != 1 {
                        continue;
                    }
                    let start = declarations[0];
                    let end = cfg_test_item_end(source, start);
                    let declaration = &source[start..end];
                    pending.extend(constant_identifiers(declaration));
                    constants.push_str(declaration);
                }
                definitions
                    .entry(name.clone())
                    .or_default()
                    .push(format!("{signature}{{{body}}}{constants}"));
            }
        }
    }
    definitions
}

#[derive(Debug, Eq, PartialEq)]
struct SemanticSourceIndex {
    calls: std::collections::BTreeSet<String>,
    definitions: std::collections::BTreeMap<String, Vec<String>>,
}

fn source_identifier_tokens(source: &str) -> Vec<(usize, &str)> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0_usize;
    let mut line_comment = false;
    let mut block_comment_depth = 0_u32;
    let mut string = false;
    let mut character = false;
    let mut escaped = false;
    while index < bytes.len() {
        if line_comment {
            line_comment = bytes[index] != b'\n';
            index += 1;
            continue;
        }
        if block_comment_depth != 0 {
            if bytes.get(index..index + 2) == Some(b"/*") {
                block_comment_depth += 1;
                index += 2;
            } else if bytes.get(index..index + 2) == Some(b"*/") {
                block_comment_depth -= 1;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if string || character {
            let byte = bytes[index];
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if (string && byte == b'"') || (character && byte == b'\'') {
                string = false;
                character = false;
            }
            index += 1;
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"//") {
            line_comment = true;
            index += 2;
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"/*") {
            block_comment_depth = 1;
            index += 2;
            continue;
        }
        if bytes[index] == b'"' {
            string = true;
            index += 1;
            continue;
        }
        if bytes[index] == b'\'' && source_char_literal_start(bytes, index) {
            character = true;
            index += 1;
            continue;
        }
        if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            index += 1;
            while bytes
                .get(index)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            {
                index += 1;
            }
            tokens.push((start, &source[start..index]));
        } else {
            index += 1;
        }
    }
    tokens
}

fn semantic_source_index(source: &str) -> SemanticSourceIndex {
    use std::collections::BTreeMap;

    let tokens = source_identifier_tokens(source);
    let mut constants = BTreeMap::<String, Vec<usize>>::new();
    for pair in tokens.windows(2) {
        let [(start, keyword), (name_start, name)] = pair else {
            unreachable!()
        };
        if matches!(*keyword, "const" | "static") && *name_start == *start + keyword.len() + 1 {
            constants
                .entry((*name).to_owned())
                .or_default()
                .push(*start);
        }
    }

    let mut definitions = BTreeMap::<String, Vec<String>>::new();
    for pair in tokens.windows(2) {
        let [(start, keyword), (name_start, name)] = pair else {
            unreachable!()
        };
        if *keyword != "fn" || *name_start != *start + "fn ".len() {
            continue;
        }
        let tail = &source[*start..];
        let Some(open) = tail.find('{') else {
            continue;
        };
        let mut brackets = 0_u32;
        let mut declaration = true;
        for byte in tail[..open].bytes() {
            match byte {
                b'[' => brackets += 1,
                b']' => brackets = brackets.saturating_sub(1),
                b';' if brackets == 0 => {
                    declaration = false;
                    break;
                }
                _ => {}
            }
        }
        if !declaration {
            continue;
        }
        let body = function_body(tail, name);
        let mut attached_constants = String::new();
        let mut pending = constant_identifiers(body);
        let mut included = std::collections::BTreeSet::new();
        while let Some(constant) = pending.pop() {
            if !included.insert(constant.clone()) {
                continue;
            }
            let Some(start) = constants
                .get(&constant)
                .filter(|declarations| declarations.len() == 1)
                .and_then(|declarations| declarations.first())
            else {
                continue;
            };
            let declaration = &source[*start..cfg_test_item_end(source, *start)];
            pending.extend(constant_identifiers(declaration));
            attached_constants.push_str(declaration);
        }
        definitions
            .entry((*name).to_owned())
            .or_default()
            .push(format!("{}{{{body}}}{attached_constants}", &tail[..open]));
    }

    SemanticSourceIndex {
        calls: called_function_names_from_tokens(source, &tokens),
        definitions,
    }
}

fn semantic_function_definitions_from_indexes(
    indexes: &[SemanticSourceIndex],
    names: &std::collections::BTreeSet<String>,
) -> std::collections::BTreeMap<String, Vec<String>> {
    names
        .iter()
        .filter_map(|name| {
            let locations = indexes
                .iter()
                .flat_map(|index| index.definitions.get(name).into_iter().flatten().cloned())
                .collect::<Vec<_>>();
            (!locations.is_empty()).then(|| (name.clone(), locations))
        })
        .collect()
}

fn semantic_function_definitions_for_names(
    corpus: &[String],
    names: &std::collections::BTreeSet<String>,
) -> std::collections::BTreeMap<String, Vec<String>> {
    if std::env::var_os("PB08_CUSTODY_REFERENCE_ANALYSIS").is_some() {
        return semantic_function_definitions_for_names_reference(corpus, names);
    }
    let indexes = corpus
        .iter()
        .map(|source| semantic_source_index(source))
        .collect::<Vec<_>>();
    semantic_function_definitions_from_indexes(&indexes, names)
}

#[test]
fn pb08_semantic_source_index_matches_reference_and_content_identity() {
    use std::collections::BTreeSet;

    fn assert_equivalent(label: &str, source: &str) {
        let index = semantic_source_index(source);
        let names = index
            .calls
            .iter()
            .chain(index.definitions.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let reference =
            semantic_function_definitions_for_names_reference(&[source.to_owned()], &names);
        assert_eq!(
            reference, index.definitions,
            "source index drifted for {label}"
        );
    }

    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let tests_root = manifest_dir.join("tests");
    for relative in [
        "cas_admission.rs",
        "cow_locality.rs",
        "operation_concurrency.rs",
        "operation_create.rs",
        "operation_faults.rs",
        "operation_lifecycle.rs",
        "operation_mutation.rs",
        "operation_read.rs",
        "support/mod.rs",
        "support/counting_sink.rs",
        "support/counting_source.rs",
        "support/fault_injection.rs",
        "support/temp_fs_cas.rs",
        "reference/naive_fastcdc.rs",
    ] {
        let source = std::fs::read_to_string(tests_root.join(relative))
            .unwrap_or_else(|error| panic!("read indexed source {relative}: {error}"));
        assert_equivalent(relative, &source);
    }
    for relative in [
        "cas/mod.rs",
        "pack/mod.rs",
        "cow/mod.rs",
        "content/mod.rs",
        "content/update.rs",
        "lifecycle/mod.rs",
        "read/extraction.rs",
        "object/mod.rs",
        "limits.rs",
    ] {
        let source = std::fs::read_to_string(manifest_dir.join("src").join(relative))
            .unwrap_or_else(|error| panic!("read indexed adapter {relative}: {error}"));
        assert_equivalent(relative, &source);
    }

    let inventory = std::fs::read_to_string(manifest_dir.join(
        "../../../ephemeral-sandbox-docs/ephemelra-sanbdox-v2.1/layefs/l1.5.5/pb08-custody-inventory.tsv",
    ))
    .expect("read PB-08 inventory for source-index regression");
    for old_file in inventory
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| line.split('\t').next())
        .collect::<BTreeSet<_>>()
    {
        let object = format!(
            "434f0f82e9a7913625d9c2916b3be26b69c64acb:crates/layerfs-storage/tests/{old_file}"
        );
        let output = std::process::Command::new("git")
            .current_dir(manifest_dir)
            .args(["show", object.as_str()])
            .output()
            .unwrap_or_else(|error| panic!("read indexed historical source {old_file}: {error}"));
        assert!(
            output.status.success(),
            "historical source unavailable: {old_file}"
        );
        let source = String::from_utf8(output.stdout).expect("historical source is UTF-8");
        assert_equivalent(old_file, &source);
    }

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time after epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "layerfs-pb08-source-index-{}-{nonce}.rs",
        std::process::id()
    ));
    std::fs::write(
        &path,
        "fn row(){first(1)}fn first(value:u64){assert_eq!(value,1)}",
    )
    .expect("write first same-path source snapshot");
    let first = semantic_source_index(
        &std::fs::read_to_string(&path).expect("read first same-path source snapshot"),
    );
    std::fs::write(
        &path,
        "fn row(){second(2)}fn second(value:u64){assert_eq!(value,2)}",
    )
    .expect("write second same-path source snapshot");
    let second = semantic_source_index(
        &std::fs::read_to_string(&path).expect("read second same-path source snapshot"),
    );
    std::fs::remove_file(&path).expect("remove same-path source fixture");
    assert_ne!(first, second, "source index reused stale same-path bytes");
    assert!(first.calls.contains("first") && !first.calls.contains("second"));
    assert!(second.calls.contains("second") && !second.calls.contains("first"));
}

fn named_call_arguments<'a>(source: &'a str, name: &str) -> Vec<Vec<&'a str>> {
    let needle = format!("{name}(");
    source
        .match_indices(&needle)
        .filter_map(|(start, _)| {
            let bounded = source
                .as_bytes()
                .get(start.wrapping_sub(1))
                .is_none_or(|byte| !(byte.is_ascii_alphanumeric() || *byte == b'_'));
            if !bounded || source[..start].trim_end().ends_with("fn") {
                return None;
            }
            call_arguments_at(source, start + name.len()).map(|(arguments, _)| arguments)
        })
        .collect()
}

fn function_parameter_names(definition: &str, name: &str) -> Vec<String> {
    let Some((signature, _)) = function_definitions(definition, name).into_iter().next() else {
        return Vec::new();
    };
    let Some(open) = signature.find('(') else {
        return Vec::new();
    };
    let Some((parameters, _)) = call_arguments_at(signature, open) else {
        return Vec::new();
    };
    parameters
        .into_iter()
        .filter_map(|parameter| {
            let binding = parameter.split(':').next().unwrap_or(parameter);
            (!binding
                .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                .any(|token| token == "self"))
            .then(|| semantic_identifiers(binding, false).into_iter().last())
            .flatten()
        })
        .collect()
}

fn substitute_parameters(
    source: &str,
    substitutions: &std::collections::BTreeMap<String, &str>,
) -> String {
    let bytes = source.as_bytes();
    let mut output = String::with_capacity(source.len());
    let mut index = 0_usize;
    let mut line_comment = false;
    let mut block_comment_depth = 0_u32;
    let mut string = false;
    let mut character = false;
    let mut escaped = false;
    while index < bytes.len() {
        if line_comment {
            output.push(bytes[index] as char);
            line_comment = bytes[index] != b'\n';
            index += 1;
            continue;
        }
        if block_comment_depth != 0 {
            if bytes.get(index..index + 2) == Some(b"/*") {
                output.push_str("/*");
                block_comment_depth += 1;
                index += 2;
            } else if bytes.get(index..index + 2) == Some(b"*/") {
                output.push_str("*/");
                block_comment_depth -= 1;
                index += 2;
            } else {
                output.push(bytes[index] as char);
                index += 1;
            }
            continue;
        }
        if string || character {
            let byte = bytes[index];
            output.push(byte as char);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if (string && byte == b'"') || (character && byte == b'\'') {
                string = false;
                character = false;
            }
            index += 1;
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"//") {
            output.push_str("//");
            line_comment = true;
            index += 2;
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"/*") {
            output.push_str("/*");
            block_comment_depth = 1;
            index += 2;
            continue;
        }
        if bytes[index] == b'"' {
            output.push('"');
            string = true;
            index += 1;
            continue;
        }
        if bytes[index] == b'\'' && source_char_literal_start(bytes, index) {
            output.push('\'');
            character = true;
            index += 1;
            continue;
        }
        if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            index += 1;
            while bytes
                .get(index)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            {
                index += 1;
            }
            let identifier = &source[start..index];
            let previous = (0..start)
                .rev()
                .find(|position| !bytes[*position].is_ascii_whitespace());
            let next = bytes[index..]
                .iter()
                .copied()
                .find(|byte| !byte.is_ascii_whitespace());
            let selector = previous.is_some_and(|position| {
                bytes[position] == b'.'
                    || (bytes[position] == b':'
                        && bytes[..position]
                            .iter()
                            .rev()
                            .copied()
                            .find(|byte| !byte.is_ascii_whitespace())
                            == Some(b':'))
            }) || next == Some(b':');
            if !selector {
                if let Some(replacement) = substitutions.get(identifier) {
                    output.push('(');
                    output.push_str(replacement.trim());
                    output.push(')');
                    continue;
                }
            }
            {
                output.push_str(identifier);
            }
            continue;
        }
        output.push(bytes[index] as char);
        index += 1;
    }
    output
}

fn definition_reaches_assertion(
    name: &str,
    definitions: &std::collections::BTreeMap<String, Vec<String>>,
    visiting: &mut std::collections::BTreeSet<String>,
    memo: &mut std::collections::BTreeMap<String, bool>,
) -> bool {
    if let Some(reaches) = memo.get(name) {
        return *reaches;
    }
    if !visiting.insert(name.to_owned()) {
        return false;
    }
    let reaches = definitions.get(name).is_some_and(|locations| {
        locations.len() == 1 && {
            let body = function_body(&locations[0], name);
            !assertion_expressions(body).is_empty()
                || called_function_names(body).into_iter().any(|called| {
                    definition_reaches_assertion(&called, definitions, visiting, memo)
                })
        }
    });
    visiting.remove(name);
    memo.insert(name.to_owned(), reaches);
    reaches
}

fn instantiated_helper_assertions(
    root: &str,
    definitions: &std::collections::BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    fn visit(
        source: &str,
        definitions: &std::collections::BTreeMap<String, Vec<String>>,
        stack: &mut std::collections::BTreeSet<String>,
        memo: &mut std::collections::BTreeMap<String, bool>,
        seen_calls: &mut std::collections::BTreeSet<(String, Vec<String>)>,
        expansions: &mut std::collections::BTreeMap<(String, Vec<String>), String>,
        assertions: &mut Vec<String>,
    ) {
        for name in called_function_names(source) {
            if stack.contains(&name)
                || !definition_reaches_assertion(
                    &name,
                    definitions,
                    &mut std::collections::BTreeSet::new(),
                    memo,
                )
            {
                continue;
            }
            let Some(locations) = definitions
                .get(&name)
                .filter(|locations| locations.len() == 1)
            else {
                continue;
            };
            let definition = &locations[0];
            let parameters = function_parameter_names(definition, &name);
            for arguments in named_call_arguments(source, &name) {
                if parameters.len() != arguments.len() {
                    continue;
                }
                let normalized_arguments = arguments
                    .iter()
                    .map(|argument| normalized_semantic_source(argument))
                    .collect::<Vec<_>>();
                let call = (name.clone(), normalized_arguments.clone());
                if !seen_calls.insert(call) {
                    continue;
                }
                let expansion = (definition.clone(), normalized_arguments);
                let body = expansions
                    .entry(expansion)
                    .or_insert_with(|| {
                        let substitutions = parameters
                            .iter()
                            .cloned()
                            .zip(arguments)
                            .collect::<std::collections::BTreeMap<_, _>>();
                        substitute_parameters(function_body(definition, &name), &substitutions)
                    })
                    .clone();
                assertions.extend(assertion_expressions(&body));
                stack.insert(name.clone());
                visit(
                    &body,
                    definitions,
                    stack,
                    memo,
                    seen_calls,
                    expansions,
                    assertions,
                );
                stack.remove(&name);
            }
        }
    }

    let mut assertions = Vec::new();
    visit(
        root,
        definitions,
        &mut std::collections::BTreeSet::new(),
        &mut std::collections::BTreeMap::new(),
        &mut std::collections::BTreeSet::new(),
        &mut std::collections::BTreeMap::new(),
        &mut assertions,
    );
    assertions
}

fn unique_function_definitions(
    candidates: std::collections::BTreeMap<String, Vec<String>>,
) -> std::collections::BTreeMap<String, Vec<String>> {
    candidates
        .into_iter()
        .filter_map(|(name, locations)| {
            let bodies = locations
                .iter()
                .map(|location| normalized_semantic_source(function_body(location, &name)))
                .collect::<std::collections::BTreeSet<_>>();
            (bodies.len() == 1).then(|| (name, vec![locations[0].clone()]))
        })
        .collect()
}

fn closed_function_definitions_for_names(
    corpus: &[String],
    names: &std::collections::BTreeSet<String>,
) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut closed = std::collections::BTreeMap::<String, Vec<String>>::new();
    for source in corpus {
        let definitions =
            semantic_function_definitions_for_names(std::slice::from_ref(source), names);
        let resolved = definitions
            .into_iter()
            .filter_map(|(name, locations)| {
                let bodies = locations
                    .iter()
                    .map(|location| normalized_semantic_source(function_body(location, &name)))
                    .collect::<std::collections::BTreeSet<_>>();
                (bodies.len() == 1).then(|| (name, vec![locations[0].clone()]))
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        for (name, locations) in &resolved {
            closed
                .entry(name.clone())
                .or_default()
                .push(transitive_semantic_source(&locations[0], &resolved));
        }
    }
    closed
}

fn reachable_function_definitions(
    roots: impl IntoIterator<Item = String>,
    definitions: &std::collections::BTreeMap<String, Vec<String>>,
) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut reachable = std::collections::BTreeMap::new();
    let mut pending = roots.into_iter().collect::<Vec<_>>();
    while let Some(name) = pending.pop() {
        if reachable.contains_key(&name) {
            continue;
        }
        let Some(locations) = definitions
            .get(&name)
            .filter(|locations| locations.len() == 1)
        else {
            continue;
        };
        pending.extend(
            called_function_names(function_body(&locations[0], &name))
                .into_iter()
                .filter(|called| definitions.contains_key(called)),
        );
        reachable.insert(name, locations.clone());
    }
    reachable
}

fn row_scoped_function_definitions(
    source: &str,
    body: &str,
    local: &std::collections::BTreeMap<String, Vec<String>>,
    adapter_definitions_by_family: &[(&str, std::collections::BTreeMap<String, Vec<String>>)],
    support_definitions: &std::collections::BTreeMap<String, Vec<String>>,
) -> std::collections::BTreeMap<String, Vec<String>> {
    use std::collections::BTreeMap;

    let local_evidence = transitive_semantic_source(body, &local);
    let row_calls = called_function_names(&local_evidence);
    let mut candidates = BTreeMap::<String, Vec<String>>::new();
    for (reachable, locations) in reachable_function_definitions(
        row_calls
            .iter()
            .filter(|name| local.contains_key(*name))
            .cloned(),
        local,
    ) {
        candidates.entry(reachable).or_default().extend(locations);
    }
    for (reachable, locations) in reachable_function_definitions(
        row_calls
            .iter()
            .filter(|name| support_definitions.contains_key(*name))
            .cloned(),
        support_definitions,
    ) {
        candidates.entry(reachable).or_default().extend(locations);
    }
    for (family, family_definitions) in adapter_definitions_by_family {
        for (reachable, locations) in reachable_function_definitions(
            row_calls
                .iter()
                .filter(|name| {
                    family_definitions.contains_key(*name)
                        && qualification_function_is_imported(source, family, name)
                })
                .cloned(),
            family_definitions,
        ) {
            candidates.entry(reachable).or_default().extend(locations);
        }
    }
    unique_function_definitions(candidates)
}

fn transitive_semantic_source(
    root: &str,
    definitions: &std::collections::BTreeMap<String, Vec<String>>,
) -> String {
    fn visit(
        source: &str,
        definitions: &std::collections::BTreeMap<String, Vec<String>>,
        stack: &mut std::collections::BTreeSet<String>,
        expansions: &mut std::collections::BTreeMap<(String, Vec<String>), String>,
        evidence: &mut String,
    ) {
        for name in called_function_names(source) {
            if stack.contains(&name) {
                continue;
            }
            let Some(definition) = definitions
                .get(&name)
                .filter(|locations| locations.len() == 1)
                .map(|locations| &locations[0])
            else {
                continue;
            };
            let Some((signature, body)) =
                function_definitions(definition, &name).into_iter().next()
            else {
                continue;
            };
            let parameters = function_parameter_names(definition, &name);
            for arguments in named_call_arguments(source, &name) {
                if parameters.len() != arguments.len() {
                    continue;
                }
                let normalized_arguments = arguments
                    .iter()
                    .map(|argument| normalized_semantic_source(argument))
                    .collect::<Vec<_>>();
                let instantiated = expansions
                    .entry((definition.clone(), normalized_arguments))
                    .or_insert_with(|| {
                        let substitutions = parameters
                            .iter()
                            .cloned()
                            .zip(arguments)
                            .collect::<std::collections::BTreeMap<_, _>>();
                        substitute_parameters(body, &substitutions)
                    })
                    .clone();
                evidence.push_str(signature);
                evidence.push('{');
                evidence.push_str(&instantiated);
                evidence.push('}');
                if let Some(function_end) = definition
                    .find('{')
                    .and_then(|open| brace_end(definition, open))
                {
                    evidence.push_str(&definition[function_end..]);
                }
                stack.insert(name.clone());
                visit(&instantiated, definitions, stack, expansions, evidence);
                stack.remove(&name);
            }
        }
    }

    let mut evidence = root.to_owned();
    visit(
        root,
        definitions,
        &mut std::collections::BTreeSet::new(),
        &mut std::collections::BTreeMap::new(),
        &mut evidence,
    );
    evidence
}

fn contains_any_semantic(source: &str, markers: &[&str]) -> bool {
    markers
        .iter()
        .any(|marker| source.contains(&normalized_semantic_source(marker)))
        || {
            let source = source.to_ascii_lowercase();
            markers.iter().any(|marker| {
                source.contains(&normalized_semantic_source(marker).to_ascii_lowercase())
            })
        }
}

fn final_semantic_marker_present(row: &[&str], column: usize, marker: &str, source: &str) -> bool {
    let normalized_marker = normalized_semantic_source(marker);
    if source.contains(&normalized_marker) || marker == row[1] || marker == row[3] {
        return true;
    }

    let lower = marker.to_ascii_lowercase();
    match column {
        7 => match marker {
            "FsCas control seam" => contains_any_semantic(
                source,
                &["FsCasV1", "TempFsCas", "create_new", "open_existing"],
            ),
            "support/mod.rs" => contains_any_semantic(source, &["support", "TempFsCas"]),
            _ => false,
        },
        8 => {
            let concepts: &[(&[&str], &[&str])] = &[
                (&["cleanup"], &["cleanup", "terminal"]),
                (&["admission"], &["admission", "admit", "operation_slots"]),
                (&["publication"], &["publication", "catalog", "visible"]),
                (&["invalidation"], &["invalidat"]),
                (
                    &["handoff"],
                    &[
                        "handoff",
                        "validated_handoff",
                        "completed_operations",
                        "PackAdmissionObservationV1::Installed",
                        "carriers_installed",
                    ],
                ),
                (&["cancellation", "cancel"], &["cancel"]),
                (&["deadline"], &["deadline"]),
                (&["preparation"], &["preparation", "spool"]),
                (&["validation"], &["validat"]),
                (&["filesystem"], &["filesystem", "fault"]),
                (&["residue"], &["residue", "retained"]),
            ];
            concepts.iter().any(|(needles, evidence)| {
                needles.iter().any(|needle| lower.contains(needle))
                    && contains_any_semantic(source, evidence)
            }) || (lower.contains("boundary")
                && contains_any_semantic(
                    source,
                    &[
                        "boundary",
                        "fault_injected",
                        "handoff",
                        "blocked",
                        "locator_publications",
                    ],
                ))
                || (lower.contains("cleanuptarget")
                    && contains_any_semantic(source, &["cleanup", "terminal"]))
        }
        9 => {
            if marker == "Cancelled" {
                return source.contains("::Cancelled");
            }
            if marker == "Deadline" {
                return source.contains("::Deadline");
            }
            let concepts: &[(&[&str], &[&str])] = &[
                (
                    &["mpsc", "thread"],
                    &[
                        "blocked",
                        "overlap",
                        "simultaneous",
                        "concurrent",
                        "wait",
                        "worker",
                        "queued",
                        "gated",
                    ],
                ),
                (&["condvar"], &["probe", "wait", "blocked"]),
                (
                    &["barrier"],
                    &[
                        "barrier",
                        "gated",
                        "blocked",
                        "simultaneous",
                        "observed_admission_high_water",
                    ],
                ),
                (
                    &["reopened"],
                    &["open_existing", "reopen", "stale_usable", "root_usable"],
                ),
                (&["slow"], &["slow", "blocked", "wait"]),
                (
                    &["simultaneous"],
                    &["simultaneous", "overlap", "concurrent"],
                ),
                (&["cancel"], &["cancel"]),
                (&["deadline"], &["deadline"]),
            ];
            concepts.iter().any(|(needles, evidence)| {
                needles.iter().any(|needle| lower.contains(needle))
                    && contains_any_semantic(source, evidence)
            })
        }
        10 => marker.rsplit_once("::").is_some_and(|(_, variant)| {
            let bounded_equivalent = match variant {
                "MissingOccupant" => contains_any_semantic(source, &["missing_occupant_cases"]),
                "ResourceExhausted" => contains_any_semantic(
                    source,
                    &[
                        "AdmissionRefusalObservationV1::StorageBytes",
                        "AdmissionRefusalObservationV1::StorageInodes",
                    ],
                ),
                "CleanupFailed" => contains_any_semantic(
                    source,
                    &[
                        "PublicationErrorV1::CleanupFailed",
                        "PublicationCauseV1::CleanupFailed",
                    ],
                ),
                "Busy" => contains_any_semantic(source, &["busy", "reopen_invalidated"]),
                "SinkRefused" => contains_any_semantic(source, &["stale_closure_refused"]),
                "ResourceRefused" => contains_any_semantic(
                    source,
                    &[
                        "over_capacity_refused",
                        "duplicate_resource_refused",
                        "slot_overflow_refused",
                    ],
                ),
                "InvalidationFailed" => contains_any_semantic(
                    source,
                    &[
                        "PublicationErrorV1::InvalidationFailed",
                        "PublicationCauseV1::InvalidationFailed",
                    ],
                ),
                "SynchronizationPoisoned" => contains_any_semantic(source, &["poisoned"]),
                _ => false,
            };
            bounded_equivalent
                || matches!(variant, "Core" | "FsCas")
                    && contains_any_semantic(source, &["error", "terminal"])
                || source.contains(&format!("::{variant}"))
                || source
                    .to_ascii_lowercase()
                    .contains(&variant.to_ascii_lowercase())
        }),
        11 => {
            if matches!(marker, "ResourceLedgerV1" | "OperationCountersV1") {
                return contains_any_semantic(
                    source,
                    &[
                        "counter",
                        "budget",
                        "resource",
                        "storage_equations",
                        "zero_forbidden_work",
                        "read_fault_matrix",
                        "terminal_optional_observations_v1",
                        "storage_terminal",
                        "bytes_read",
                        "bytes_written",
                        "admitted_slots",
                        "operation_slots",
                        "zero_before",
                        "zero_after",
                    ],
                );
            }
            if marker == "record_rayon_work" {
                return source.contains("rayon_work_units");
            }
            if marker == "record_workspace_sized_staging_allocation" {
                return source.contains("workspace_sized_staging_allocations");
            }
            if marker == "ClosureSource" {
                return contains_any_semantic(source, &["closure", "stale_closure_refused"]);
            }
            if matches!(
                marker,
                "to_be_bytes" | "from_be_bytes" | "to_le_bytes" | "as_bytes"
            ) {
                return contains_any_semantic(
                    source,
                    &["bytes", "canonical", "decode", "encode", "digest", "_id"],
                );
            }

            let mut recognized = false;
            let mut present = false;
            for (needles, evidence) in [
                (
                    &["counter"][..],
                    &[
                        "counter",
                        "storage_equations",
                        "zero_forbidden_work",
                        "read_fault_matrix",
                        "terminal_optional_observations_v1",
                        "storage_terminal",
                        "bytes_read",
                        "bytes_written",
                        "admitted_slots",
                        "operation_slots",
                        "zero_before",
                        "zero_after",
                        "committed_objects",
                        "tree_nodes_created",
                        "memory_high_water",
                    ][..],
                ),
                (
                    &["bytes"][..],
                    &[
                        "bytes",
                        "_len",
                        "storage",
                        "residue",
                        "payload",
                        "wrapper_mode",
                        "wrapper_entry_count",
                        "wrapper_root_page_absent",
                    ][..],
                ),
                (
                    &["calls"][..],
                    &["calls", "polls", "operations", "acquisitions"][..],
                ),
                (
                    &["slot"][..],
                    &["slot", "authority_clean", "admission", "operation_slots"][..],
                ),
                (&["residue"][..], &["residue", "retained", "cleanup"][..]),
                (&["read"][..], &["read", "extract", "decode"][..]),
                (&["write"][..], &["write", "sink", "storage"][..]),
                (
                    &["memory", "budget"][..],
                    &["memory", "budget", "resource", "planned"][..],
                ),
                (
                    &["high_water"][..],
                    &["high_water", "maximum", "bounded"][..],
                ),
                (
                    &["forbidden"][..],
                    &[
                        "forbidden",
                        "zero_forbidden_work",
                        "rayon_work_units",
                        "workspace_sized_staging_allocations",
                    ][..],
                ),
                (
                    &["reopen"][..],
                    &["open_existing", "reopen", "root_usable"][..],
                ),
                (
                    &["admission", "admitted"][..],
                    &["admission", "admit", "authority_clean"][..],
                ),
                (&["preparation"][..], &["preparation", "spool"][..]),
                (&["carrier"][..], &["carrier", "immutable"][..]),
                (&["locator"][..], &["locator", "publication"][..]),
                (&["catalog"][..], &["catalog", "publication"][..]),
                (
                    &["source", "supplier"][..],
                    &["source", "supplier", "input"][..],
                ),
                (&["reuse"][..], &["reuse", "dedup", "incumbent"][..]),
                (
                    &["candidate", "loser", "contender"][..],
                    &["candidate", "loser", "contender"][..],
                ),
                (&["winner", "incumbent"][..], &["winner", "incumbent"][..]),
                (&["observed"][..], &["observed", "observation", "polls"][..]),
            ] {
                if needles.iter().any(|needle| lower.contains(needle)) {
                    recognized = true;
                    present |= contains_any_semantic(source, evidence);
                }
            }
            if recognized {
                present
            } else if lower.starts_with("run_") {
                contains_any_semantic(
                    source,
                    &["complete", "create", "supplier", "operation", "publication"],
                )
            } else {
                false
            }
        }
        12 => {
            (lower.contains("unix")
                && contains_any_semantic(
                    source,
                    &[
                        "cfg(unix)",
                        "unix",
                        "symlink",
                        "device",
                        "inode",
                        "Unavailable",
                        "NotApplicable",
                    ],
                ))
                || (lower.contains("apple")
                    && contains_any_semantic(source, &["apple", "apfs", "Unavailable"]))
        }
        _ => false,
    }
}

fn missing_semantic_markers(row: &[&str], evidence: &str) -> Vec<String> {
    let normalized = normalized_semantic_source(evidence);
    row[7..13]
        .iter()
        .enumerate()
        .flat_map(|(offset, field)| {
            field
                .split(';')
                .map(move |marker| (offset + 7, marker.trim()))
        })
        .filter(|(_, marker)| !matches!(*marker, "" | "none"))
        .filter(|(column, marker)| {
            !final_semantic_marker_present(row, *column, marker, &normalized)
        })
        .map(|(_, marker)| marker.to_owned())
        .collect()
}

fn attributable_semantic_markers(row: &[&str], evidence: &str) -> Vec<(usize, String)> {
    let normalized = normalized_semantic_source(evidence);
    row[7..13]
        .iter()
        .enumerate()
        .flat_map(|(offset, field)| {
            field
                .split(';')
                .map(move |marker| (offset + 7, marker.trim()))
        })
        .filter(|(_, marker)| !matches!(*marker, "" | "none"))
        .filter(|(column, marker)| final_semantic_marker_present(row, *column, marker, &normalized))
        .map(|(column, marker)| (column, marker.to_owned()))
        .collect()
}

fn digest_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    support::sha256(bytes)
        .into_iter()
        .flat_map(|byte| {
            [
                char::from(DIGITS[usize::from(byte >> 4)]),
                char::from(DIGITS[usize::from(byte & 0x0f)]),
            ]
        })
        .collect()
}

fn qualification_exports(source: &str) -> Vec<String> {
    let source = normalized_semantic_source(source);
    let mut exports = source
        .match_indices("pubusecrate::")
        .map(|(start, _)| {
            let end = source[start..]
                .find(';')
                .map(|offset| start + offset + 1)
                .expect("qualification reexport terminates");
            source[start..end].to_owned()
        })
        .collect::<Vec<_>>();
    exports.sort();
    exports
}

fn qualification_public_items(source: &str, module: &str) -> Vec<String> {
    let start = source
        .find(module)
        .unwrap_or_else(|| panic!("missing qualification module {module}"));
    let end = cfg_test_item_end(source, start);
    let source = normalized_semantic_source(&source[start..end]);
    let mut items = Vec::new();
    let mut offset = 0_usize;
    let patterns = [
        ("pubconstfn", true),
        ("pubfn", true),
        ("pubstruct", false),
        ("pubenum", false),
        ("pubtrait", false),
        ("pubtype", false),
        ("pubconst", false),
        ("pubstatic", false),
        ("pubuse", false),
    ];
    while offset < source.len() {
        let Some((start, pattern, signature_only)) = patterns
            .iter()
            .filter_map(|(pattern, signature_only)| {
                source[offset..]
                    .find(pattern)
                    .map(|relative| (offset + relative, *pattern, *signature_only))
            })
            .min_by_key(|(start, _, _)| *start)
        else {
            break;
        };
        let delimiter = source[start..]
            .find(['{', ';'])
            .map(|relative| start + relative)
            .unwrap_or_else(|| panic!("unterminated public item {pattern}"));
        let end = if signature_only || source.as_bytes()[delimiter] == b';' {
            delimiter + usize::from(!signature_only)
        } else {
            let mut depth = 0_u32;
            source[delimiter..]
                .char_indices()
                .find_map(|(relative, character)| match character {
                    '{' => {
                        depth += 1;
                        None
                    }
                    '}' => {
                        depth -= 1;
                        (depth == 0).then_some(delimiter + relative + 1)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("unterminated public item {pattern}"))
        };
        items.push(source[start..end].to_owned());
        offset = if signature_only { delimiter + 1 } else { end };
    }
    items
}

fn contains_identifier(source: &str, identifier: &str) -> bool {
    source.match_indices(identifier).any(|(start, _)| {
        let end = start + identifier.len();
        let bytes = source.as_bytes();
        bytes
            .get(start.wrapping_sub(1))
            .is_none_or(|byte| !(byte.is_ascii_alphanumeric() || *byte == b'_'))
            && bytes
                .get(end)
                .is_none_or(|byte| !(byte.is_ascii_alphanumeric() || *byte == b'_'))
    })
}

fn production_source_v1(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut output = String::with_capacity(source.len());
    let mut copy_start = 0_usize;
    let mut index = 0_usize;
    let mut braces = 0_u32;
    let mut line_comment = false;
    let mut block_comment_depth = 0_u32;
    let mut string = false;
    let mut character = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if line_comment {
            if byte == b'\n' {
                line_comment = false;
            }
            index += 1;
            continue;
        }
        if block_comment_depth != 0 {
            if bytes.get(index..index + 2) == Some(b"/*") {
                block_comment_depth += 1;
                index += 2;
            } else if bytes.get(index..index + 2) == Some(b"*/") {
                block_comment_depth -= 1;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
            index += 1;
            continue;
        }
        if character {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'\'' {
                character = false;
            }
            index += 1;
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"//") {
            line_comment = true;
            index += 2;
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"/*") {
            block_comment_depth = 1;
            index += 2;
            continue;
        }
        if braces == 0 && bytes.get(index..index + 6) == Some(b"#[cfg(") {
            let attribute_end = source_attribute_end(source, index + 1);
            let attribute = &source[index..attribute_end];
            if attribute.contains("test") || attribute.contains("operation-polymorphism") {
                output.push_str(&source[copy_start..index]);
                index = cfg_test_item_end(source, attribute_end);
                copy_start = index;
                continue;
            }
        }
        match byte {
            b'"' => string = true,
            b'\'' if source_char_literal_start(bytes, index) => character = true,
            b'{' => braces += 1,
            b'}' => {
                assert!(braces != 0, "unbalanced production source at byte {index}");
                braces -= 1;
            }
            _ => {}
        }
        index += 1;
    }
    output.push_str(&source[copy_start..]);
    output
}

fn forbidden_content_import_v1(source: &str) -> Option<&'static str> {
    const FORBIDDEN: &[&str] = &[
        "crate::cas",
        "crate::pack",
        "crate::lifecycle",
        "std::fs",
        "std::path",
        "FsCas",
        "FsOperationSpool",
        "FileClosureObjectSpool",
        "FileGlobalSeenSpool",
        "FilePackIndexSpool",
        "CompletedPackSetV1",
        "FsPrivatePack",
        "FsCarrier",
        "FsLocator",
        "FsCatalog",
        "CatalogMarker",
        "LocatorIndex",
        "OperationPreparationV1",
        "PreparationV1",
        "DirectPack",
        "hard_link",
    ];
    let mut import = String::new();
    let mut in_import = false;
    for line in source.lines() {
        let trimmed = line.trim_start();
        if !in_import
            && (trimmed.starts_with("use ")
                || trimmed.starts_with("pub use ")
                || trimmed.starts_with("pub(crate) use "))
        {
            import.clear();
            in_import = true;
        }
        if in_import {
            import.push_str(trimmed);
            if trimmed.contains(';') {
                if let Some(forbidden) = FORBIDDEN.iter().find(|value| import.contains(**value)) {
                    return Some(forbidden);
                }
                in_import = false;
            }
        }
    }
    None
}

fn forbidden_final_owner_dispatch(source: &str) -> Option<&'static str> {
    [
        "qualification::run(",
        "ScenarioV1",
        "numeric_dispatch",
        "mod c3_",
    ]
    .into_iter()
    .find(|token| source.contains(token))
}

fn support_helper_is_load_bearing(source: &str, helper: &str) -> bool {
    match helper {
        "TempFsCas" => source.contains("TempFsCas::new(") && source.contains(".path()"),
        "CountingSource" => {
            source.contains("CountingSource::new(")
                && source.contains("update_from_reader_v1(")
                && source.contains("&mut source")
        }
        "CountingSink" => {
            source.contains("CountingSink::new(")
                && source.contains("read_v1_to_writer(")
                && source.contains("&mut sink")
        }
        "FaultPoint" => {
            source.contains("FaultPoint::cancel_at(")
                && source.contains("let mut should_cancel = || fault.observe(1)")
                && source.contains("&mut should_cancel")
        }
        _ => false,
    }
}

// Active PB-06 implementation evidence only. This compact map intentionally
// stays beside the architecture seam so the source/test custody is visible in
// the current tree; it is not benchmark, qualification, or final-custody
// evidence.
struct Pb06BoundaryTraceabilityV1 {
    boundary: &'static str,
    source_path: &'static str,
    source_symbol: &'static str,
    test_path: &'static str,
    test_symbol: &'static str,
    assertion_markers: &'static [&'static str],
}

const PB06_BOUNDARY_TRACEABILITY_V1: &[Pb06BoundaryTraceabilityV1] = &[
    Pb06BoundaryTraceabilityV1 {
        boundary: "publication/no-replace",
        source_path: "src/cas/fs.rs",
        source_symbol: "publish_small_marker_controlled",
        test_path: "tests/cas_admission.rs",
        test_symbol: "partial_multi_object_locator_publication_is_fully_rolled_back",
        assertion_markers: &["PublicationErrorV1::Core(CoreError::Cancelled)", "read_dir"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "incumbent/equality/collision",
        source_path: "src/cas/locator.rs",
        source_symbol: "decide_persistent_locator_install_v1",
        test_path: "tests/cas_admission.rs",
        test_symbol: "existing_catalog_classifies_valid_binding_and_unequal_incumbents",
        assertion_markers: &[
            "PublicationErrorV1::UnequalOccupant",
            "unreachable_installed_residue_bytes",
        ],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "rollback custody",
        source_path: "src/cas/fs.rs",
        source_symbol: "rollback_unpublished_admission",
        test_path: "tests/operation_lifecycle.rs",
        test_symbol: "locator_rollback_preserves_directional_unlink_faults_and_dependency_custody",
        assertion_markers: &[
            "PublicationCleanupTargetV1::ObjectLocator",
            "storage_bytes_retained",
        ],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "restart/wrong operation-generation-incarnation deletion attempt",
        source_path: "src/cas/fs.rs",
        source_symbol: "rollback_unpublished_admission",
        test_path: "src/cas/fs.rs",
        test_symbol: "reopened_incarnation_reusing_numeric_nonce_cannot_rollback_earlier_locator",
        assertion_markers: &["spawn_worker", "assert_eq!(after, before)", "after_usage"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "file-backed maximum probe",
        source_path: "src/cas/locator_index.rs",
        source_symbol: "lookup",
        test_path: "src/cas/locator_index.rs",
        test_symbol: "file_backed_index_reaches_the_real_maximum_collision_probe",
        assertion_markers: &["GLOBAL_SEEN_MAXIMUM_PROBES_PER_LOOKUP_V1", "maximum_probe"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "cleanup/residue custody",
        source_path: "src/cas/fs.rs",
        source_symbol: "retain_all_live_v1",
        test_path: "tests/operation_lifecycle.rs",
        test_symbol: "locator_cleanup_unwind_attempts_every_remaining_locator_and_carrier_once",
        assertion_markers: &["observation.cleanup_calls()", "storage_bytes_retained"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "invalidation",
        source_path: "src/cas/fs.rs",
        source_symbol: "invalidate_root_controlled_v1",
        test_path: "tests/operation_lifecycle.rs",
        test_symbol:
            "post_link_alias_cleanup_failure_retains_visible_dependencies_and_invalidates_reopen",
        assertion_markers: &[
            "observation.reopen_invalidated()",
            "storage_inodes_retained",
        ],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "cross-carrier lookup",
        source_path: "src/cas/fs.rs",
        source_symbol: "gather_object_locator_incumbent_evidence",
        test_path: "tests/operation_faults.rs",
        test_symbol:
            "cross_carrier_object_validation_read_failures_are_typed_and_cleanup_the_candidate",
        assertion_markers: &["assert_read_fault_matrix(observation, 24, 6)"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "simultaneous same-key publication",
        source_path: "src/cas/fs.rs",
        source_symbol: "publish_small_marker_controlled",
        test_path: "tests/operation_concurrency.rs",
        test_symbol: "simultaneous_reopened_pack_callers_publish_one_canonical_shared_locator",
        assertion_markers: &["shared_id_matches", "PublicationOutcomeV1::Installed"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "locator prepare/install/revalidate/cleanup faults",
        source_path: "src/cas/fs.rs",
        source_symbol: "install_object_locators",
        test_path: "tests/cas_admission.rs",
        test_symbol: "every_fresh_admission_boundary_cleans_or_counts_exact_residue",
        assertion_markers: &[
            "case.after_catalog_publication()",
            "case.residue_bytes()",
            "case.expected_residue_bytes()",
        ],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "post-validation locator replacement",
        source_path: "src/cas/fs.rs",
        source_symbol: "rollback_unpublished_admission",
        test_path: "src/cas/fs.rs",
        test_symbol: "locator_rollback_rejects_foreign_replacement_after_final_validation",
        assertion_markers: &["control.replaced", "held-locator-during-rollback"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "post-validation carrier replacement",
        source_path: "src/cas/fs.rs",
        source_symbol: "rollback_unpublished_carrier",
        test_path: "src/cas/fs.rs",
        test_symbol: "carrier_rollback_rejects_foreign_replacement_after_final_validation",
        assertion_markers: &["control.replaced", "held-carrier-during-rollback"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "catalog adoption before rollback",
        source_path: "src/cas/fs.rs",
        source_symbol: "rollback_unpublished_admission",
        test_path: "src/cas/fs.rs",
        test_symbol: "catalog_adoption_before_rollback_retains_the_complete_dependency_chain",
        assertion_markers: &[
            "control.adopted",
            "decode_catalog_marker",
            "exact_namespace_usage",
        ],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "locator revalidation before visibility",
        source_path: "src/cas/fs.rs",
        source_symbol: "revalidate_active_pack_marker_incumbent_controlled_v1",
        test_path: "tests/cas_admission.rs",
        test_symbol: "post_comparison_locator_path_replacement_fails_before_catalog_publication",
        assertion_markers: &[
            "observation.catalog_entries()",
            "PublicationErrorV1::Integrity",
            "observation.reopen_invalidated()",
        ],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "exact locator rollback policy",
        source_path: "src/cas/locator.rs",
        source_symbol: "decide_persistent_locator_rollback_v1",
        test_path: "src/cas/locator.rs",
        test_symbol: "locator_rollback_policy_requires_exact_receipt_and_current_operation",
        assertion_markers: &["Authorized", "Foreign", "snapshot_matches"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "combined frozen byte compatibility",
        source_path: "src/cas/fs.rs",
        source_symbol: "frozen_compatibility_all_five_byte_domains_round_trip_and_hash_exactly",
        test_path: "src/cas/fs.rs",
        test_symbol: "frozen_compatibility_all_five_byte_domains_round_trip_and_hash_exactly",
        assertion_markers: &[
            "read_generation_marker",
            "decode_existing_root_owner",
            "digest_hex",
        ],
    },
];

fn test_is_registered(source: &str, manifest: &str, test_path: &str, test_symbol: &str) -> bool {
    let signature = format!("fn {test_symbol}");
    let Some(start) = source.find(&signature) else {
        return false;
    };
    let prefix = &source[..start];
    let Some(attribute) = prefix.rfind("#[test]") else {
        return false;
    };
    !prefix[attribute + "#[test]".len()..].contains("fn ")
        && (test_path.starts_with("src/")
            || cargo_test_targets(manifest)
                .iter()
                .any(|(_, path, _)| path == test_path))
}

#[test]
fn pb06_boundary_to_test_traceability_is_explicit_and_current() {
    let source_files = [
        ("src/lib.rs", include_str!("../src/lib.rs")),
        ("src/cas/fs.rs", include_str!("../src/cas/fs.rs")),
        ("src/cas/locator.rs", include_str!("../src/cas/locator.rs")),
        (
            "src/cas/locator_index.rs",
            include_str!("../src/cas/locator_index.rs"),
        ),
    ];
    let test_files = [
        ("tests/cas_admission.rs", include_str!("cas_admission.rs")),
        (
            "tests/operation_concurrency.rs",
            include_str!("operation_concurrency.rs"),
        ),
        (
            "tests/operation_faults.rs",
            include_str!("operation_faults.rs"),
        ),
        (
            "tests/operation_lifecycle.rs",
            include_str!("operation_lifecycle.rs"),
        ),
        ("src/cas/fs.rs", include_str!("../src/cas/fs.rs")),
        ("src/cas/locator.rs", include_str!("../src/cas/locator.rs")),
        (
            "src/cas/locator_index.rs",
            include_str!("../src/cas/locator_index.rs"),
        ),
    ];
    let registration_source = include_str!("../Cargo.toml");

    assert_eq!(
        PB06_BOUNDARY_TRACEABILITY_V1.len(),
        16,
        "the PB-06 map must retain one explicit row for every required proof boundary"
    );
    for row in PB06_BOUNDARY_TRACEABILITY_V1 {
        let source = source_files
            .iter()
            .find(|(path, _)| *path == row.source_path)
            .map(|(_, content)| *content)
            .unwrap_or_else(|| panic!("missing mapped source file {}", row.source_path));
        function_body(source, row.source_symbol);

        let tests = test_files
            .iter()
            .find(|(path, _)| *path == row.test_path)
            .map(|(_, content)| *content)
            .unwrap_or_else(|| panic!("missing mapped test file {}", row.test_path));
        let test_body = function_body(tests, row.test_symbol);
        assert!(
            test_is_registered(tests, registration_source, row.test_path, row.test_symbol,),
            "PB-06 boundary {} names an unregistered test {} in {}",
            row.boundary,
            row.test_symbol,
            row.test_path
        );
        for marker in row.assertion_markers {
            assert!(
                test_body.contains(marker),
                "PB-06 boundary {} lost assertion marker {} in {}::{}",
                row.boundary,
                marker,
                row.test_path,
                row.test_symbol
            );
        }
    }
}

#[test]
fn workspace_shape_and_package_boundaries_are_stable() {
    let workspace = include_str!("../../../Cargo.toml");
    assert!(workspace.contains("resolver = \"2\""));
    for member in [
        "crates/layerfs-sdk",
        "crates/layerfs-storage",
        "crates/layerfs-driver",
    ] {
        assert!(
            workspace.contains(&format!("\"{member}\"")),
            "missing member {member}"
        );
    }
    assert_eq!(workspace.matches("    \"crates/").count(), 3);

    let sdk = include_str!("../../layerfs-sdk/Cargo.toml");
    let storage = include_str!("../Cargo.toml");
    let driver = include_str!("../../layerfs-driver/Cargo.toml");

    assert!(sdk.contains("name = \"layerfs-sdk\""));
    assert!(sdk.contains("name = \"layerfs\""));
    assert!(sdk.contains("publish = true"));
    assert!(storage.contains("name = \"layerfs-storage\""));
    assert!(storage.contains("publish = false"));
    assert!(driver.contains("name = \"layerfs-driver\""));
    assert!(driver.contains("publish = false"));

    let storage_dependencies = section(storage, "dependencies");
    assert!(storage_dependencies.contains("blake3.workspace = true"));
    assert_eq!(
        storage_dependencies
            .lines()
            .filter(|line| {
                let line = line.trim();
                !line.is_empty() && !line.starts_with('#')
            })
            .count(),
        1,
        "BLAKE3 must be the sole private L1 runtime dependency"
    );
    let driver_dependencies = section(driver, "dependencies");
    assert!(driver_dependencies.contains("layerfs-storage"));
    assert!(!driver_dependencies.contains("layerfs-sdk"));
    let sdk_dependencies = section(sdk, "dependencies");
    assert!(sdk_dependencies.contains("layerfs-driver"));
    assert!(sdk_dependencies.contains("layerfs-storage"));
    assert!(!storage.contains("layerfs-driver"));
    assert!(!driver.contains("layerfs-sdk"));
}

#[test]
fn prohibited_runtime_dependencies_are_not_present() {
    let workspace = include_str!("../../../Cargo.toml");
    let storage = include_str!("../Cargo.toml");
    assert!(workspace.contains("blake3 = { version = \"=1.8.5\", default-features = false }"));
    assert!(section(storage, "dependencies").contains("blake3.workspace = true"));
    for forbidden in [
        "serde", "bincode", "opendal", "git2", "fuser", "wasmtime", "oci",
    ] {
        assert!(
            !workspace.to_ascii_lowercase().contains(forbidden),
            "forbidden dependency {forbidden}"
        );
    }
}

#[test]
fn fscas_remains_private_to_the_unpublished_storage_implementation() {
    let storage_manifest = include_str!("../Cargo.toml");
    let sdk = include_str!("../../layerfs-sdk/src/lib.rs");
    let driver = include_str!("../../layerfs-driver/src/lib.rs");

    assert!(storage_manifest.contains("publish = false"));
    for public_surface in [sdk, driver] {
        for private_name in [
            "FsCas",
            "fscas",
            "FsPrivatePack",
            "CompleteValidatedClosure",
        ] {
            assert!(
                !public_surface.contains(private_name),
                "private storage name {private_name} leaked into an SDK/driver surface"
            );
        }
    }
}

#[test]
fn storage_source_follows_the_domain_responsibility_map() {
    use std::path::Path;

    let lib = include_str!("../src/lib.rs");
    for forbidden_module in [
        "mod c3;",
        "mod cas_stream;",
        "mod fscas;",
        "mod tree;",
        "mod update;",
    ] {
        assert!(
            !lib.contains(forbidden_module),
            "legacy ownership module remains in lib.rs: {forbidden_module}"
        );
    }

    let required_domain_files = [
        include_str!("../src/error.rs"),
        include_str!("../src/limits.rs"),
        include_str!("../src/profile.rs"),
        include_str!("../src/identity/mod.rs"),
        include_str!("../src/identity/framing.rs"),
        include_str!("../src/identity/logical.rs"),
        include_str!("../src/identity/physical.rs"),
        include_str!("../src/cdc/mod.rs"),
        include_str!("../src/cdc/engine.rs"),
        include_str!("../src/cdc/resync.rs"),
        include_str!("../src/cdc/fastcdc/mod.rs"),
        include_str!("../src/cdc/fastcdc/scanner.rs"),
        include_str!("../src/cdc/fastcdc/gear.rs"),
        include_str!("../src/cdc/fastcdc/rejoin.rs"),
        include_str!("../src/cdc/seqcdc/mod.rs"),
        include_str!("../src/cdc/seqcdc/scanner.rs"),
        include_str!("../src/cdc/seqcdc/rejoin.rs"),
        include_str!("../src/format/mod.rs"),
        include_str!("../src/format/codec.rs"),
        include_str!("../src/format/path.rs"),
        include_str!("../src/object/mod.rs"),
        include_str!("../src/object/model.rs"),
        include_str!("../src/object/encode.rs"),
        include_str!("../src/object/decode.rs"),
        include_str!("../src/object/port_decode.rs"),
        include_str!("../src/object/traversal.rs"),
        include_str!("../src/content/mod.rs"),
        include_str!("../src/content/file.rs"),
        include_str!("../src/content/create.rs"),
        include_str!("../src/content/replace.rs"),
        include_str!("../src/content/update.rs"),
        include_str!("../src/content/read.rs"),
        include_str!("../src/cas/mod.rs"),
        include_str!("../src/cas/port.rs"),
        include_str!("../src/cas/fs.rs"),
        include_str!("../src/cas/admission.rs"),
        include_str!("../src/cas/catalog.rs"),
        include_str!("../src/cas/closure.rs"),
        include_str!("../src/cas/closure_storage.rs"),
        include_str!("../src/cas/locator.rs"),
        include_str!("../src/cas/locator_index.rs"),
        include_str!("../src/cas/operation_admission.rs"),
        include_str!("../src/lifecycle/mod.rs"),
        include_str!("../src/lifecycle/preparation.rs"),
        include_str!("../src/pack/mod.rs"),
        include_str!("../src/pack/complete_writer.rs"),
        include_str!("../src/pack/operation_index.rs"),
        include_str!("../src/read/mod.rs"),
        include_str!("../src/read/extraction.rs"),
        include_str!("../src/read/range.rs"),
        include_str!("../src/read/object_reader.rs"),
        include_str!("../src/cow/mod.rs"),
        include_str!("../src/cow/file.rs"),
        include_str!("../src/cow/tree.rs"),
        include_str!("../src/cow/view.rs"),
        include_str!("../src/cow/mutate.rs"),
        include_str!("../src/bin/c3_qualification.rs"),
    ];
    assert!(
        required_domain_files
            .iter()
            .all(|source| !source.is_empty()),
        "a required domain ownership file is empty"
    );

    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for prohibited in ["object.rs", "traversal.rs", "pack.rs", "lifecycle.rs", "c3"] {
        assert_path_absent(&source_root.join(prohibited));
    }
}

#[test]
fn pb08_final_custody_tree_and_targets_are_exact() {
    let source_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let expected_source_files = [
        "bin/c3_qualification.rs",
        "cas/admission.rs",
        "cas/catalog.rs",
        "cas/closure.rs",
        "cas/closure_storage.rs",
        "cas/fs.rs",
        "cas/locator.rs",
        "cas/locator_index.rs",
        "cas/mod.rs",
        "cas/operation_admission.rs",
        "cas/port.rs",
        "cdc/engine.rs",
        "cdc/fastcdc/gear.rs",
        "cdc/fastcdc/mod.rs",
        "cdc/fastcdc/rejoin.rs",
        "cdc/fastcdc/scanner.rs",
        "cdc/mod.rs",
        "cdc/resync.rs",
        "cdc/seqcdc/mod.rs",
        "cdc/seqcdc/rejoin.rs",
        "cdc/seqcdc/scanner.rs",
        "content/create.rs",
        "content/file.rs",
        "content/mod.rs",
        "content/read.rs",
        "content/replace.rs",
        "content/update.rs",
        "cow/file.rs",
        "cow/mod.rs",
        "cow/mutate.rs",
        "cow/tree.rs",
        "cow/view.rs",
        "error.rs",
        "format/codec.rs",
        "format/mod.rs",
        "format/path.rs",
        "identity/framing.rs",
        "identity/logical.rs",
        "identity/mod.rs",
        "identity/physical.rs",
        "lib.rs",
        "lifecycle/mod.rs",
        "lifecycle/preparation.rs",
        "limits.rs",
        "object/decode.rs",
        "object/encode.rs",
        "object/mod.rs",
        "object/model.rs",
        "object/port_decode.rs",
        "object/traversal.rs",
        "pack/complete_writer.rs",
        "pack/mod.rs",
        "pack/operation_index.rs",
        "profile.rs",
        "read/extraction.rs",
        "read/mod.rs",
        "read/object_reader.rs",
        "read/range.rs",
    ];
    let mut expected_source_files = expected_source_files
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    expected_source_files.sort();
    assert_eq!(
        relative_files(&source_root),
        expected_source_files,
        "PB-08 production custody tree changed"
    );

    let tests_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let expected_test_files = [
        "cas_admission.rs",
        "cow_locality.rs",
        "fixtures/c3-registry-v1.tsv",
        "l0_architecture.rs",
        "operation_concurrency.rs",
        "operation_create.rs",
        "operation_faults.rs",
        "operation_lifecycle.rs",
        "operation_mutation.rs",
        "operation_read.rs",
        "reference/naive_fastcdc.rs",
        "support/counting_sink.rs",
        "support/counting_source.rs",
        "support/fault_injection.rs",
        "support/mod.rs",
        "support/temp_fs_cas.rs",
    ];
    let mut expected_test_files = expected_test_files
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    expected_test_files.sort();
    assert_eq!(
        relative_files(&tests_root),
        expected_test_files,
        "PB-08 integration-test custody tree changed"
    );

    let manifest = include_str!("../Cargo.toml");
    assert!(manifest.contains("autobins = false"));
    assert!(manifest.contains("autotests = false"));
    assert_eq!(manifest.matches("[[test]]").count(), 9);
    assert_eq!(manifest.matches("[[bin]]").count(), 0);
    let expected_targets = [
        ("l0_architecture", "tests/l0_architecture.rs", None),
        ("operation_create", "tests/operation_create.rs", None),
        ("cas_admission", "tests/cas_admission.rs", None),
        ("operation_lifecycle", "tests/operation_lifecycle.rs", None),
        (
            "operation_concurrency",
            "tests/operation_concurrency.rs",
            None,
        ),
        ("operation_faults", "tests/operation_faults.rs", None),
        ("operation_mutation", "tests/operation_mutation.rs", None),
        ("operation_read", "tests/operation_read.rs", None),
        ("cow_locality", "tests/cow_locality.rs", None),
    ];
    let actual_targets = cargo_test_targets(manifest);
    assert_eq!(actual_targets.len(), expected_targets.len());
    for ((name, path, required), (expected_name, expected_path, expected_required)) in
        actual_targets.iter().zip(expected_targets)
    {
        assert_eq!(name, expected_name);
        assert_eq!(path, expected_path);
        assert_eq!(required.as_deref(), expected_required);
    }
    for forbidden in [
        "c3_fscas",
        "c3_operation",
        "c3_mutation",
        "c3_seqcdc",
        "l0_codec_vectors",
        "l1_tree",
        "l155_qualification",
    ] {
        assert!(
            !manifest.contains(forbidden),
            "retired target remains: {forbidden}"
        );
    }
}

#[test]
fn pb08_final_sources_are_substantive_and_alias_free() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let source_root = manifest_dir.join("src");
    for relative in relative_files(&source_root) {
        let path = source_root.join(&relative);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read production source {path:?}: {error}"));
        assert!(
            !source.trim().is_empty(),
            "empty production source: {relative}"
        );
        assert!(
            !source.contains("#[path =") && !source.contains("include!("),
            "production source uses a migration alias in {relative}"
        );
        assert!(!source.contains("src/c3/") && !source.contains("src/provider/"));
        assert!(!source.contains("l155_qualification"));
        assert!(
            !production_source_v1(&source).trim().is_empty(),
            "production logic hidden entirely behind cfg(test): {relative}"
        );
    }

    let tests_root = manifest_dir.join("tests");
    for relative in relative_files(&tests_root) {
        let path = tests_root.join(&relative);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read test source {path:?}: {error}"));
        assert!(
            !source.trim().is_empty(),
            "empty final test/support file: {relative}"
        );
        if relative != "l0_architecture.rs" {
            let exact_reference_import = relative == "operation_create.rs"
                && source
                    .matches("#[path = \"reference/naive_fastcdc.rs\"]")
                    .count()
                    == 1;
            assert!(
                exact_reference_import
                    || (!source.contains("#[path =") && !source.contains("include!(")),
                "final test tree uses a migration alias in {relative}"
            );
            if exact_reference_import {
                assert_eq!(source.matches("#[path =").count(), 1);
            }
        }
    }

    for (relative, required) in [
        (
            "support/temp_fs_cas.rs",
            ["TempFsCas", "create_dir", "path"].as_slice(),
        ),
        (
            "support/counting_source.rs",
            ["CountingSource", "pub fn read", "bytes_read"].as_slice(),
        ),
        (
            "support/counting_sink.rs",
            [
                "CountingSink",
                "pub fn begin",
                "pub fn finish",
                "pub fn abort",
            ]
            .as_slice(),
        ),
        (
            "support/fault_injection.rs",
            ["FaultPoint", "cancel_at", "pub fn observe"].as_slice(),
        ),
        (
            "reference/naive_fastcdc.rs",
            [
                "pub fn cut",
                "pub fn ends",
                "const GEAR",
                "const MINIMUM_CHUNK_BYTES",
            ]
            .as_slice(),
        ),
    ] {
        let source = std::fs::read_to_string(tests_root.join(relative)).unwrap();
        for marker in required {
            assert!(
                source.contains(marker),
                "{relative} lost substantive marker {marker}"
            );
        }
    }
    let reference = std::fs::read_to_string(tests_root.join("reference/naive_fastcdc.rs")).unwrap();
    for forbidden in [
        "layerfs_storage::",
        "ChunkerSpecV1",
        "canonical_bytes",
        "profile::",
        "cdc::",
    ] {
        assert!(
            !reference.contains(forbidden),
            "FastCDC reference is production-derived through {forbidden}"
        );
    }

    let final_owners = [
        "cas_admission.rs",
        "cow_locality.rs",
        "operation_concurrency.rs",
        "operation_create.rs",
        "operation_faults.rs",
        "operation_lifecycle.rs",
        "operation_mutation.rs",
        "operation_read.rs",
    ];
    let owner_sources = final_owners
        .into_iter()
        .map(|relative| {
            (
                relative,
                std::fs::read_to_string(tests_root.join(relative)).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let create_source = owner_sources
        .iter()
        .find_map(|(relative, source)| (*relative == "operation_create.rs").then_some(source))
        .expect("create owner source exists");
    assert_eq!(
        create_source
            .matches("#[path = \"reference/naive_fastcdc.rs\"]")
            .count(),
        1,
        "the exact independent FastCDC reference must be imported once"
    );
    assert_eq!(
        create_source.matches("crate::naive_fastcdc::ends").count(),
        2,
        "the imported FastCDC reference must be exercised by frozen comparisons"
    );
    assert!(
        create_source.contains("[16_688, 34_949, 52_688, 70_914, 90_807, 100_000]"),
        "FastCDC comparison lost its pinned boundary corpus"
    );
    for (relative, expected_tests, expected_feature_gates) in [
        ("cas_admission.rs", 50, 1),
        ("cow_locality.rs", 16, 1),
        ("operation_concurrency.rs", 23, 1),
        ("operation_create.rs", 46, 3),
        ("operation_faults.rs", 74, 1),
        ("operation_lifecycle.rs", 31, 1),
        ("operation_mutation.rs", 13, 3),
        ("operation_read.rs", 9, 2),
    ] {
        let source = owner_sources
            .iter()
            .find_map(|(candidate, source)| (*candidate == relative).then_some(source))
            .expect("final owner source exists");
        assert_eq!(
            source.matches("#[test]").count(),
            expected_tests,
            "final owner test custody count drifted: {relative}"
        );
        assert_eq!(
            source
                .matches("#[cfg(feature = \"operation-polymorphism\")]")
                .count(),
            expected_feature_gates,
            "feature applicability gate count drifted: {relative}"
        );
    }
    for (relative, source) in &owner_sources {
        assert!(
            source.contains("#[test]"),
            "final owner has no tests: {relative}"
        );
        assert!(
            source.contains("assert") || source.contains("panic!"),
            "final owner has no semantic assertions: {relative}"
        );
        if let Some(forbidden) = forbidden_final_owner_dispatch(source) {
            panic!("final owner {relative} retained forwarding/dispatcher token {forbidden}");
        }
    }
    for (support, owner) in [
        ("TempFsCas", "cas_admission.rs"),
        ("CountingSource", "operation_mutation.rs"),
        ("CountingSink", "operation_read.rs"),
        ("FaultPoint", "operation_faults.rs"),
    ] {
        let source = owner_sources
            .iter()
            .find_map(|(relative, source)| (*relative == owner).then_some(source))
            .expect("owner source exists");
        assert!(
            support_helper_is_load_bearing(source, support),
            "support helper {support} is not load-bearing in {owner}"
        );
    }
    assert!(forbidden_final_owner_dispatch(
        "fn test() { qualification::run(ScenarioV1::new(7)); }"
    )
    .is_some());
    let faults = owner_sources
        .iter()
        .find_map(|(relative, source)| (*relative == "operation_faults.rs").then_some(source))
        .expect("fault owner source exists");
    assert!(!support_helper_is_load_bearing(
        &faults.replace("&mut should_cancel", "|| true"),
        "FaultPoint"
    ));
    let mutation = owner_sources
        .iter()
        .find_map(|(relative, source)| (*relative == "operation_mutation.rs").then_some(source))
        .expect("mutation owner source exists");
    assert!(!support_helper_is_load_bearing(
        &mutation.replace("&mut source", "&mut std::io::Cursor::new(inserted)"),
        "CountingSource"
    ));
    let read = owner_sources
        .iter()
        .find_map(|(relative, source)| (*relative == "operation_read.rs").then_some(source))
        .expect("read owner source exists");
    assert!(!support_helper_is_load_bearing(
        &read.replace("&mut sink", "&mut std::io::sink()"),
        "CountingSink"
    ));
    let lib = std::fs::read_to_string(source_root.join("lib.rs")).unwrap();
    assert!(!lib.contains("#[test]"));
    assert!(!lib.contains("ScenarioV1"));
    assert!(!lib.contains("numeric_dispatch"));
    assert!(
        !lib.contains("semantic::*") && !lib.contains("resources::*"),
        "qualification facade must enumerate its approved symbols"
    );
    assert!(
        lib.lines().count() < 220,
        "qualification facade grew into a test repository"
    );
    let exports = qualification_exports(&lib);
    assert_eq!(exports.len(), 6, "qualification module families drifted");
    assert_eq!(
        exports
            .iter()
            .map(|export| export.split_once('{').expect("braced reexport").0)
            .collect::<Vec<_>>(),
        [
            "pubusecrate::cas::semantic::",
            "pubusecrate::content::semantic::",
            "pubusecrate::cow::semantic::",
            "pubusecrate::lifecycle::semantic::",
            "pubusecrate::limits::resources::",
            "pubusecrate::pack::semantic::",
        ]
    );
    let export_names = exports
        .iter()
        .flat_map(|export| {
            export
                .split_once('{')
                .and_then(|(_, names)| names.strip_suffix("};"))
                .expect("braced qualification reexport")
                .split(',')
                .filter(|name| !name.is_empty())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        export_names.len(),
        262,
        "qualification export count drifted"
    );
    assert!(export_names.iter().all(|name| !name.contains(['*', ' '])));
    let normalized_exports = exports.join("\n");
    assert_eq!(
        digest_hex(normalized_exports.as_bytes()),
        "8a6fc64be5bfc6a6622fb108d5391716f861733d778a5b80c84d69ad379b0778",
        "qualification export allowlist drifted"
    );
    assert_ne!(
        digest_hex(
            normalized_exports
                .replacen("admit_v1", "admit_v2", 1)
                .as_bytes()
        ),
        "8a6fc64be5bfc6a6622fb108d5391716f861733d778a5b80c84d69ad379b0778",
        "qualification export allowlist is not mutation-sensitive"
    );
    let owner_corpus = owner_sources
        .iter()
        .map(|(_, source)| source.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for name in &export_names {
        assert!(
            contains_identifier(&owner_corpus, name)
                || matches!(
                    *name,
                    "CompleteMutationCountersV1" | "MutationReadCrossingObservationV1"
                ),
            "qualification export is not owned by a final scenario or its result shape: {name}"
        );
    }

    let semantic_modules = [
        ("cas/mod.rs", "pub mod semantic {"),
        ("content/mod.rs", "pub mod semantic {"),
        ("content/update.rs", "pub mod semantic {"),
        ("cow/mod.rs", "pub mod semantic {"),
        ("lifecycle/mod.rs", "pub mod semantic {"),
        ("limits.rs", "pub mod resources {"),
        ("pack/mod.rs", "pub mod semantic {"),
    ];
    let mut public_items = semantic_modules
        .iter()
        .flat_map(|(relative, module)| {
            let source = std::fs::read_to_string(source_root.join(relative))
                .unwrap_or_else(|error| panic!("read {relative}: {error}"));
            qualification_public_items(&source, module)
        })
        .collect::<Vec<_>>();
    public_items.sort();
    let public_surface = public_items.join("\n");
    assert_eq!(
        digest_hex(public_surface.as_bytes()),
        "86726e16d7ce9e13ba4ceccb1d3cef5a6bee1318e39937f08c28d5eb6c875670",
        "qualification transitive public surface drifted ({} declarations)",
        public_items.len()
    );
    // Borrowed paths are the approved root/path request port. Read and Write
    // are approved only for the two bounded, caller-owned streaming seams.
    assert!(public_surface.contains("&Path"));
    assert_eq!(public_surface.matches("&mutdynstd::io::Read").count(), 1);
    assert_eq!(public_surface.matches("&mutdynstd::io::Write").count(), 1);
    assert_eq!(public_surface.matches("dyn").count(), 2);
    for concrete in [
        "CanonicalDirectoryTreeV1",
        "CompletedPackSetV1",
        "FsCasV1",
        "FsOperationSpoolV1",
        "FsPrivatePackV1",
        "OperationHandoffV1",
        "OperationMemoryPlanV1",
        "ResourceLedgerV1",
        "StorageSessionV1",
    ] {
        assert!(
            !public_surface.contains(concrete),
            "concrete storage type leaked through qualification: {concrete}"
        );
    }
    let create_source = owner_sources
        .iter()
        .find_map(|(relative, source)| {
            (*relative == "operation_create.rs").then_some(source.as_str())
        })
        .expect("operation_create owner exists");
    let resources_source = create_source
        .split_once("mod l1_resources {")
        .and_then(|(_, source)| source.split_once("\nmod l1_content {"))
        .map(|(source, _)| source)
        .expect("operation_create resource owner exists");
    assert!(
        resources_source.contains("layerfs_storage::qualification::resources"),
        "resource owner lost its bounded semantic adapter"
    );
    for forbidden in [
        "OperationCountersV1",
        "OperationMemoryPlanV1",
        "MemoryComponentV1",
        "ResourceLedgerV1",
        "limits::ResourceLedgerV1",
    ] {
        assert!(
            !resources_source.contains(forbidden),
            "resource owner reached through concrete/internal symbol {forbidden}"
        );
    }
    assert!(!tests_root.join("reference/naive_seqcdc.rs").exists());
}

struct Pb08HistoricalRowAnalysis {
    _evidence: String,
    assertions: Vec<String>,
    markers: Vec<(usize, String)>,
    bridge_graph: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
}

struct Pb08FinalRowAnalysis {
    _scoped_definitions: std::collections::BTreeMap<String, Vec<String>>,
    evidence: String,
    _normalized_evidence: String,
    assertions: Vec<String>,
    _bridge_graph: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    semantic_missing: Vec<String>,
    assertion_gaps: Vec<String>,
}

struct Pb08FinalRowWork<'a> {
    owner: &'a str,
    row: &'a [&'a str],
    body_len: usize,
    definitions: std::collections::BTreeMap<String, Vec<String>>,
    evidence: String,
    normalized_evidence: String,
    assertions: Vec<String>,
    semantic_missing: Vec<String>,
}

fn mutate_named_test_source(source: &mut String, name: &str, from: &str, to: &str) {
    let (_, start, _) = final_test_segments(source)
        .into_iter()
        .find(|(candidate, _, _)| candidate == name)
        .unwrap_or_else(|| panic!("PB-08 mutation target is absent: {name}"));
    let end = cfg_test_item_end(source, start + "#[test]".len());
    let matches = source[start..end].match_indices(from).collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "PB-08 mutation target is not unique: {name}"
    );
    let start = start + matches[0].0;
    source.replace_range(start..start + from.len(), to);
}

#[test]
fn pb08_custody_inventory_is_executable_and_exact() {
    use std::collections::{BTreeMap, BTreeSet};

    let profile = std::env::var_os("PB08_CUSTODY_PROFILE").is_some();
    let profile_start = std::time::Instant::now();
    let mut profile_last = profile_start;
    let mut profile_phase = |phase: &str| {
        if profile {
            let now = std::time::Instant::now();
            eprintln!(
                "PB08 profile: {phase}: phase={:.3}s total={:.3}s",
                now.duration_since(profile_last).as_secs_f64(),
                now.duration_since(profile_start).as_secs_f64()
            );
            profile_last = now;
        }
    };
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let inventory_path = manifest_dir.join(
        "../../../ephemeral-sandbox-docs/ephemelra-sanbdox-v2.1/layefs/l1.5.5/".to_owned()
            + "pb08-custody-inventory.tsv",
    );
    let inventory = std::fs::read_to_string(&inventory_path)
        .unwrap_or_else(|error| panic!("read custody inventory {inventory_path:?}: {error}"));
    assert_eq!(
        digest_hex(inventory.as_bytes()),
        "e91f69fd92e9611b486deb23fbad25ea282d3800b8e8bfd23825857e42939fdf",
        "PB-08 custody inventory bytes drifted"
    );
    let mut lines = inventory.lines();
    let expected_header = [
        "old_file",
        "old_function",
        "final_owner",
        "final_function",
        "original_feature_applicability",
        "assertion_count",
        "assertion_sequence_sha256",
        "fixture_support_ownership",
        "fault_boundary_order",
        "race_schedule",
        "expected_typed_error",
        "counter_forbidden_work_expectations",
        "portability_qualifier",
        "exact_command_result",
    ];
    let header = lines
        .next()
        .expect("custody inventory has a header")
        .split('\t')
        .collect::<Vec<_>>();
    assert_eq!(header, expected_header.to_vec());

    let rows = lines
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            assert_eq!(
                fields.len(),
                expected_header.len(),
                "malformed inventory row"
            );
            fields
        })
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 266);

    let deferred_owner = "DEFERRED_UNTIL_L1_COMPLETE";
    let deferred_names = [
        "pinned_hand_vectors_freeze_unsigned_equal_threshold_jump_clamp_and_eof",
        "optimized_seqcdc_matches_oracle_on_hostile_corpora",
        "optimized_seqcdc_fragmentation_is_oracle_exact_and_wraps",
        "optimized_seqcdc_pause_counters_and_terminal_errors_are_exact",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let deferred = rows
        .iter()
        .filter(|row| row[2] == deferred_owner)
        .map(|row| row[3])
        .collect::<BTreeSet<_>>();
    assert_eq!(deferred, deferred_names);
    assert_eq!(
        rows.iter().filter(|row| row[2] == deferred_owner).count(),
        4
    );

    let active = rows
        .iter()
        .filter(|row| row[2] != deferred_owner)
        .collect::<Vec<_>>();
    assert_eq!(active.len(), 262);
    assert!(active
        .iter()
        .all(|row| row[4] == "default" || row[4] == "operation-polymorphism"));
    assert!(active
        .iter()
        .all(|row| row[7..13].iter().all(|field| !field.trim().is_empty())));

    let old_keys = active
        .iter()
        .map(|row| (row[0], row[1]))
        .collect::<BTreeSet<_>>();
    assert_eq!(old_keys.len(), 262, "duplicate historical custody row");
    let final_claims = active
        .iter()
        .map(|row| (row[2], row[3]))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        final_claims.len(),
        262,
        "duplicate final owner/function claim"
    );
    profile_phase("inventory and uniqueness");

    let owner_names = [
        "cas_admission.rs",
        "cow_locality.rs",
        "operation_concurrency.rs",
        "operation_create.rs",
        "operation_faults.rs",
        "operation_lifecycle.rs",
        "operation_mutation.rs",
        "operation_read.rs",
    ];
    let diagnostic_row = std::env::var("PB08_CUSTODY_ROW").ok();
    let diagnostic_owner = std::env::var("PB08_CUSTODY_OWNER").ok();
    for (name, value) in [
        ("PB08_CUSTODY_ROW", diagnostic_row.as_deref()),
        ("PB08_CUSTODY_OWNER", diagnostic_owner.as_deref()),
    ] {
        assert!(
            value.is_none_or(|value| !value.trim().is_empty()),
            "{name} must not be empty"
        );
    }
    if let Some(owner) = diagnostic_owner.as_deref() {
        assert_ne!(owner, deferred_owner, "deferred custody is not executable");
        assert!(
            owner_names.contains(&owner),
            "unknown PB-08 custody owner: {owner}"
        );
    }
    let diagnostic_row_owner = diagnostic_row.as_deref().map(|name| {
        let matches = rows.iter().filter(|row| row[3] == name).collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "unknown PB-08 custody row: {name}");
        let owner = matches[0][2];
        assert_ne!(
            owner, deferred_owner,
            "deferred custody row is not executable: {name}"
        );
        owner
    });
    if let (Some(row_owner), Some(owner)) = (diagnostic_row_owner, diagnostic_owner.as_deref()) {
        assert_eq!(
            row_owner, owner,
            "conflicting PB-08 custody selectors: row belongs to {row_owner}, owner is {owner}"
        );
    }
    let diagnostic_effective_owner = diagnostic_owner.as_deref().or(diagnostic_row_owner);
    let diagnostic = diagnostic_row.is_some() || diagnostic_owner.is_some();
    let tests_root = manifest_dir.join("tests");
    let mut current_names = BTreeMap::<&str, BTreeSet<String>>::new();
    let mut current_sources = BTreeMap::<&str, String>::new();
    for owner in owner_names {
        let source = std::fs::read_to_string(tests_root.join(owner))
            .unwrap_or_else(|error| panic!("read final owner {owner}: {error}"));
        let declarations = final_test_segments(&source);
        assert!(
            !declarations.is_empty(),
            "final owner has no tests: {owner}"
        );
        current_names.insert(
            owner,
            declarations
                .iter()
                .map(|(name, _, _)| name.to_owned())
                .collect::<BTreeSet<_>>(),
        );
        current_sources.insert(owner, source);
    }
    if let Ok(mutation) = std::env::var("PB08_CUSTODY_SOURCE_MUTATION") {
        let (owner, name, from, to) = match mutation.as_str() {
            "cancelled_error" => (
                "operation_faults.rs",
                "cancellation_during_loser_readback_keeps_incumbent_and_cleans_candidate",
                "CoreError::Cancelled",
                "CoreError::Deadline",
            ),
            "read_overlap" => (
                "operation_read.rs",
                "mutation_crosses_reopened_full_and_exact_range_reads_without_serializing_payload_delivery",
                "reopened_mutation_read_crossings_v1(root.path())",
                "missing_reopened_mutation_read_crossings_v1(root.path())",
            ),
            _ => panic!("unknown PB-08 custody source mutation: {mutation}"),
        };
        mutate_named_test_source(current_sources.get_mut(owner).unwrap(), name, from, to);
    }

    let mut expected_names = BTreeMap::<&str, BTreeSet<String>>::new();
    for row in &active {
        expected_names
            .entry(row[2])
            .or_default()
            .insert(row[3].to_owned());
    }
    for owner in owner_names {
        assert_eq!(
            current_names.get(owner).expect("current owner names"),
            expected_names.get(owner).expect("inventory owner names"),
            "inventory does not describe the exact registered tests in {owner}"
        );
    }
    for (owner, expected_count) in owner_names.into_iter().zip([50, 16, 23, 46, 74, 31, 13, 9]) {
        assert_eq!(
            current_names.get(owner).expect("current owner names").len(),
            expected_count,
            "feature-enabled discovery count drifted in {owner}"
        );
        assert_ne!(expected_count, 0, "zero-test discovery is not qualifying");
    }

    let mut expected_gated = BTreeMap::<&str, BTreeSet<String>>::new();
    for row in &active {
        if row[4] == "operation-polymorphism" {
            expected_gated
                .entry(row[2])
                .or_default()
                .insert(row[3].to_owned());
        }
    }
    for (owner, expected_count) in owner_names.into_iter().zip([50, 16, 23, 24, 74, 31, 13, 3]) {
        let actual = feature_gated_test_names(current_sources.get(owner).unwrap());
        let historical = expected_gated.get(owner).cloned().unwrap_or_default();
        assert!(
            historical.is_subset(&actual),
            "historically feature-gated tests became unconditional in {owner}"
        );
        assert_eq!(
            actual.len(),
            expected_count,
            "final feature boundary drifted in {owner}"
        );
    }
    for (owner, module) in [
        ("cow_locality.rs", "cow_owner"),
        ("operation_create.rs", "l1_resources"),
        ("operation_create.rs", "l1_content"),
        ("operation_mutation.rs", "l1_content"),
        ("operation_mutation.rs", "l1_update"),
    ] {
        assert!(
            current_sources[owner].contains(&format!(
                "#[cfg(feature = \"operation-polymorphism\")]\nmod {module}"
            )),
            "qualification-dependent module escaped the feature boundary: {owner}::{module}"
        );
    }
    profile_phase("current owners and applicability");

    let delegated_cleanup_rows = [
        "private_pack_cleanup_unwind_terminalizes_storage_and_preparation_before_return",
        "private_pack_cleanup_unwind_retains_invalidation_double_fault",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let historical_revision = "434f0f82e9a7913625d9c2916b3be26b69c64acb";
    let mut historical_sources = BTreeMap::<String, String>::new();
    for old_file in rows
        .iter()
        .filter(|row| {
            diagnostic_row
                .as_deref()
                .is_none_or(|selected| row[3] == selected)
                && diagnostic_effective_owner.is_none_or(|selected| row[2] == selected)
        })
        .map(|row| row[0])
        .collect::<BTreeSet<_>>()
    {
        let object = format!("{historical_revision}:crates/layerfs-storage/tests/{old_file}");
        let output = std::process::Command::new("git")
            .current_dir(manifest_dir)
            .args(["show", object.as_str()])
            .output()
            .unwrap_or_else(|error| panic!("read frozen historical source {old_file}: {error}"));
        assert!(
            output.status.success(),
            "frozen historical source is unavailable for {old_file}:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        historical_sources.insert(
            old_file.to_owned(),
            String::from_utf8(output.stdout).expect("historical Rust source is UTF-8"),
        );
    }
    profile_phase("historical source loads");
    let mut historical_analysis = BTreeMap::<(String, String), Pb08HistoricalRowAnalysis>::new();
    let mut historical_scoped_definitions =
        BTreeMap::<String, BTreeMap<String, Vec<String>>>::new();
    for row in &rows {
        if diagnostic_row
            .as_deref()
            .is_some_and(|diagnostic| row[3] != diagnostic)
            || diagnostic_owner
                .as_deref()
                .is_some_and(|diagnostic| row[2] != diagnostic)
        {
            continue;
        }
        let count = row[5]
            .parse::<usize>()
            .unwrap_or_else(|error| panic!("invalid assertion count in {}: {error}", row[3]));
        if row[2] != deferred_owner && count == 0 {
            assert!(
                delegated_cleanup_rows.contains(row[3]),
                "empty frozen assertion custody is not an approved delegated row: {}",
                row[3]
            );
        }
        let historical_source = historical_sources
            .get(row[0])
            .unwrap_or_else(|| panic!("historical owner is absent: {}", row[0]));
        let (_, historical_start, historical_segment) = final_test_segments(historical_source)
            .into_iter()
            .find(|(name, _, _)| name == row[1])
            .unwrap_or_else(|| panic!("frozen historical test is absent: {}::{}", row[0], row[1]));
        let assertions = assertion_macro_sequence(historical_segment);
        let assertion_digest = digest_hex(assertions.concat().as_bytes());
        assert_eq!(
            assertions.len(),
            count,
            "inventory assertion count differs from frozen source: {}::{}",
            row[0],
            row[1]
        );
        assert_eq!(
            assertion_digest, row[6],
            "inventory assertion sequence differs from frozen source: {}::{}",
            row[0], row[1]
        );
        let scope = enclosing_module_source(historical_source, historical_start).to_owned();
        let definitions = historical_scoped_definitions
            .entry(scope.clone())
            .or_insert_with(|| semantic_function_definitions(&[scope]));
        let root = function_body(historical_source, row[1]);
        let root_evidence = transitive_semantic_source(root, &definitions);
        let frozen_inventory_evidence = format!("{historical_segment}{root_evidence}");
        let historical_missing = missing_semantic_markers(row, &frozen_inventory_evidence);
        assert!(
            historical_missing.is_empty(),
            "inventory semantic fields are not derived from its frozen source segment {}::{}: {}",
            row[0],
            row[1],
            historical_missing.join(", ")
        );
        let key = (row[0].to_owned(), row[1].to_owned());
        let markers = attributable_semantic_markers(row, &root_evidence);
        let mut assertions = assertion_expressions(root);
        assertions.extend(instantiated_helper_assertions(root, &definitions));
        historical_analysis.insert(
            key,
            Pb08HistoricalRowAnalysis {
                bridge_graph: assertion_bridge_graph(&root_evidence),
                _evidence: root_evidence,
                assertions,
                markers,
            },
        );
        assert!(
            !row[13].trim().is_empty(),
            "inventory retained an empty historical command cell: {}",
            row[3]
        );
    }
    profile_phase("historical row analysis");

    let mut current_scoped_definitions = BTreeMap::<String, BTreeMap<String, Vec<String>>>::new();
    let work_calls = if let Some(row_name) = diagnostic_row.as_deref() {
        let owner = diagnostic_effective_owner.expect("validated row owner");
        let source = current_sources[owner].as_str();
        let (_, start, _) = final_test_segments(source)
            .into_iter()
            .find(|(name, _, _)| name == row_name)
            .expect("selected row declaration");
        let body = function_body(source, row_name);
        let scope = enclosing_module_source(source, start).to_owned();
        let local = current_scoped_definitions
            .entry(scope.clone())
            .or_insert_with(|| semantic_function_definitions(&[scope]));
        called_function_names(&transitive_semantic_source(body, &local))
    } else {
        called_function_names(
            &current_sources
                .iter()
                .filter(|(owner, _)| {
                    diagnostic_effective_owner.is_none_or(|selected| **owner == selected)
                })
                .map(|(_, source)| source.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        )
    };
    let adapter_specs: [(&str, &[&str]); 7] = [
        ("cas", &["cas/mod.rs"]),
        ("pack", &["pack/mod.rs"]),
        ("cow", &["cow/mod.rs"]),
        ("content", &["content/mod.rs", "content/update.rs"]),
        ("lifecycle", &["lifecycle/mod.rs", "read/extraction.rs"]),
        ("object", &["object/mod.rs"]),
        ("resources", &["limits.rs"]),
    ];
    let adapter_sources_by_family = adapter_specs
        .into_iter()
        .filter(|(family, _)| {
            !diagnostic || {
                let source =
                    current_sources[diagnostic_effective_owner.expect("diagnostic owner")].as_str();
                work_calls
                    .iter()
                    .any(|name| qualification_function_is_imported(source, family, name))
            }
        })
        .map(|(family, relatives)| {
            let sources = relatives
                .iter()
                .map(|relative| {
                    std::fs::read_to_string(manifest_dir.join("src").join(relative))
                        .unwrap_or_else(|error| panic!("read semantic adapter {relative}: {error}"))
                })
                .collect::<Vec<_>>();
            (family, sources)
        })
        .collect::<Vec<_>>();
    let mut adapter_names = work_calls.clone();
    adapter_names.extend(called_function_names(
        &adapter_sources_by_family
            .iter()
            .flat_map(|(_, sources)| sources.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
    ));
    let adapter_definitions_by_family = adapter_sources_by_family
        .iter()
        .map(|(family, sources)| {
            (
                *family,
                closed_function_definitions_for_names(sources, &adapter_names),
            )
        })
        .collect::<Vec<_>>();
    let support_sources = [
        "support/mod.rs",
        "support/counting_sink.rs",
        "support/counting_source.rs",
        "support/fault_injection.rs",
        "support/temp_fs_cas.rs",
        "reference/naive_fastcdc.rs",
    ]
    .map(|relative| {
        std::fs::read_to_string(tests_root.join(relative))
            .unwrap_or_else(|error| panic!("read final support source {relative}: {error}"))
    });
    let mut support_names = work_calls;
    support_names.extend(called_function_names(
        &support_sources
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n"),
    ));
    let support_definitions =
        closed_function_definitions_for_names(&support_sources, &support_names);
    profile_phase("adapter and support indexes");
    profile_phase("definitions by owner");
    let mut final_evidence = Vec::new();
    let mut semantic_mismatches = Vec::new();
    let mut assertion_mismatches = Vec::new();
    let mut profile_evidence = std::time::Duration::ZERO;
    let mut profile_assertions = std::time::Duration::ZERO;
    let mut profile_bridges = std::time::Duration::ZERO;
    let mut profile_matching = std::time::Duration::ZERO;
    let mut final_work = Vec::new();

    for owner in owner_names {
        let source = current_sources.get(owner).unwrap();
        let declarations = final_test_segments(source);
        for row in active.iter().filter(|row| {
            row[2] == owner
                && diagnostic_row
                    .as_deref()
                    .is_none_or(|diagnostic| row[3] == diagnostic)
                && diagnostic_owner
                    .as_deref()
                    .is_none_or(|diagnostic| row[2] == diagnostic)
        }) {
            let (_, start, _) = declarations
                .iter()
                .find(|(name, _, _)| name == row[3])
                .expect("inventory test declaration");
            let body_end = cfg_test_item_end(source, start + "#[test]".len());
            let body_segment = &source[*start..body_end];
            let body = function_body(source, row[3]);
            let row_phase = std::time::Instant::now();
            let scope = enclosing_module_source(source, *start).to_owned();
            let local = current_scoped_definitions
                .entry(scope.clone())
                .or_insert_with(|| semantic_function_definitions(&[scope]));
            let definitions = row_scoped_function_definitions(
                source,
                body,
                local,
                &adapter_definitions_by_family,
                &support_definitions,
            );
            let evidence = transitive_semantic_source(body, &definitions);
            profile_evidence += row_phase.elapsed();
            let row_phase = std::time::Instant::now();
            let key = (row[0].to_owned(), row[1].to_owned());
            let historical = &historical_analysis[&key];
            let normalized_evidence = normalized_semantic_source(&evidence);
            let semantic_missing = historical
                .markers
                .iter()
                .filter(|(column, marker)| {
                    !final_semantic_marker_present(row, *column, marker, &normalized_evidence)
                })
                .map(|(_, marker)| marker.clone())
                .collect::<Vec<_>>();
            let mut assertions = assertion_expressions(body);
            assertions.extend(instantiated_helper_assertions(body, &definitions));
            profile_assertions += row_phase.elapsed();
            assert!(
                body_segment.contains("assert")
                    || body_segment.contains("expect")
                    || body_segment.contains("unwrap")
                    || body_segment.contains("panic!"),
                "test lost executable assertion/error custody: {}",
                row[3]
            );
            final_work.push(Pb08FinalRowWork {
                owner,
                row: row.as_slice(),
                body_len: body.len(),
                definitions,
                evidence,
                normalized_evidence,
                assertions,
                semantic_missing,
            });
        }
    }
    let bridge_phase = std::time::Instant::now();
    let parallel_bridges = final_work.len() > 1
        && std::env::var_os("PB08_CUSTODY_SERIAL").is_none()
        && std::env::var_os("PB08_CUSTODY_BRIDGE_PROFILE").is_none();
    let evidence = final_work
        .iter()
        .map(|work| work.evidence.as_str())
        .collect::<Vec<_>>();
    let bridge_graphs = assertion_bridge_graphs(&evidence, parallel_bridges);
    profile_bridges += bridge_phase.elapsed();
    for (work, bridge_graph) in final_work.into_iter().zip(bridge_graphs) {
        let Pb08FinalRowWork {
            owner,
            row,
            body_len,
            definitions,
            evidence,
            normalized_evidence,
            assertions,
            semantic_missing,
        } = work;
        if std::env::var_os("PB08_CUSTODY_ROW_PROFILE").is_some() {
            eprintln!(
                "PB08 profile: row {owner}::{} evidence_bytes={} expansion_sha256={} bridge_nodes={}",
                row[3],
                evidence.len(),
                digest_hex(evidence[body_len..].as_bytes()),
                bridge_graph.len(),
            );
        }
        let key = (row[0].to_owned(), row[1].to_owned());
        let historical = &historical_analysis[&key];
        let row_phase = std::time::Instant::now();
        let assertion_gaps = assertion_contract_gaps_with_graphs(
            &historical.assertions,
            &assertions,
            &historical.bridge_graph,
            &bridge_graph,
        );
        profile_matching += row_phase.elapsed();
        let analysis = Pb08FinalRowAnalysis {
            _scoped_definitions: definitions,
            evidence,
            _normalized_evidence: normalized_evidence,
            assertions,
            _bridge_graph: bridge_graph,
            semantic_missing,
            assertion_gaps,
        };
        if !diagnostic {
            final_evidence.push(analysis.evidence.clone());
        }
        if !analysis.semantic_missing.is_empty() {
            semantic_mismatches.push(format!(
                "{owner}::{} missing {}",
                row[3],
                analysis.semantic_missing.join(", ")
            ));
        }
        if diagnostic_row.as_deref() == Some(row[3]) {
            eprintln!("historical assertions: {:#?}", historical.assertions);
            eprintln!("final assertions: {:#?}", analysis.assertions);
        }
        for gap in analysis.assertion_gaps.iter().take(if diagnostic {
            analysis.assertion_gaps.len()
        } else {
            1
        }) {
            assertion_mismatches.push(format!(
                "{owner}::{} historical={} attributable_final={}: {gap}",
                row[3],
                historical.assertions.len(),
                analysis.assertions.len()
            ));
        }
    }
    if profile {
        eprintln!(
            "PB08 profile: row detail: evidence={:.3}s assertions={:.3}s bridges={:.3}s matching={:.3}s",
            profile_evidence.as_secs_f64(),
            profile_assertions.as_secs_f64(),
            profile_bridges.as_secs_f64(),
            profile_matching.as_secs_f64()
        );
    }
    profile_phase("row comparisons");
    if std::env::var_os("PB08_CUSTODY_ORACLE").is_some()
        || std::env::var_os("PB08_CUSTODY_ORACLE_SUMMARY").is_some()
    {
        let oracle = format!(
            "semantic_mismatches={semantic_mismatches:#?}\nassertion_mismatches={assertion_mismatches:#?}"
        );
        eprintln!(
            "PB08 oracle: sha256={} bytes={} semantic={} assertion={}",
            digest_hex(oracle.as_bytes()),
            oracle.len(),
            semantic_mismatches.len(),
            assertion_mismatches.len()
        );
    }
    assert!(
        std::env::var_os("PB08_CUSTODY_ORACLE_SUMMARY").is_none(),
        "PB-08 custody oracle summary requested"
    );
    assert!(
        assertion_mismatches.is_empty(),
        "assertion custody drifted:\n{}",
        assertion_mismatches.join("\n")
    );
    assert!(
        semantic_mismatches.is_empty(),
        "semantic custody drifted:\n{}",
        semantic_mismatches.join("\n")
    );
    assert!(
        diagnostic_row.is_none() && diagnostic_owner.is_none(),
        "PB-08 custody diagnostic passed; rerun the complete gate without PB08_CUSTODY_ROW or PB08_CUSTODY_OWNER"
    );
    let final_executable_evidence = final_evidence.join("\n");
    let normalized_final_evidence = normalized_semantic_source(&final_executable_evidence);
    let mut relocated_support_mismatches = Vec::new();
    for row in &active {
        let key = (row[0].to_owned(), row[1].to_owned());
        for (offset, field) in row[7..13].iter().enumerate() {
            let column = offset + 7;
            for marker in field.split(';').map(str::trim) {
                if matches!(marker, "" | "none")
                    || historical_analysis[&key]
                        .markers
                        .contains(&(column, marker.to_owned()))
                    || final_semantic_marker_present(
                        row,
                        column,
                        marker,
                        &normalized_final_evidence,
                    )
                {
                    continue;
                }
                relocated_support_mismatches
                    .push(format!("{}::{} missing {marker}", row[0], row[1]));
            }
        }
    }
    assert!(
        relocated_support_mismatches.is_empty(),
        "relocated historical support is not load-bearing in the final executable graph:\n{}",
        relocated_support_mismatches.join("\n")
    );

    let mut evidence_for = |owner: &str, name: &str| {
        let source = current_sources[owner].as_str();
        let (_, start, _) = final_test_segments(source)
            .into_iter()
            .find(|(candidate, _, _)| candidate == name)
            .expect("final test declaration");
        let body = function_body(source, name);
        let scope = enclosing_module_source(source, start).to_owned();
        let local = current_scoped_definitions
            .entry(scope.clone())
            .or_insert_with(|| semantic_function_definitions(&[scope]));
        let definitions = row_scoped_function_definitions(
            source,
            body,
            local,
            &adapter_definitions_by_family,
            &support_definitions,
        );
        transitive_semantic_source(body, &definitions)
    };
    let has_required = |evidence: &str, markers: &[&str]| {
        let normalized = normalized_semantic_source(evidence);
        markers
            .iter()
            .all(|marker| normalized.contains(&normalized_semantic_source(marker)))
    };

    let cancellation_name =
        "cancellation_during_loser_readback_keeps_incumbent_and_cleans_candidate";
    let cancellation_row = active
        .iter()
        .find(|row| row[3] == cancellation_name)
        .expect("cancellation custody row");
    let cancellation = evidence_for("operation_faults.rs", cancellation_name);
    assert!(missing_semantic_markers(cancellation_row, &cancellation).is_empty());
    let cancellation_error = "PublicationErrorV1::Core(CoreError::Cancelled)";
    assert!(has_required(&cancellation, &[cancellation_error]));
    assert!(!has_required(
        &cancellation.replace(
            cancellation_error,
            "PublicationErrorV1::Core(CoreError::Deadline)"
        ),
        &[cancellation_error]
    ));

    let read_name =
        "mutation_crosses_reopened_full_and_exact_range_reads_without_serializing_payload_delivery";
    let read_adapter_name = "reopened_mutation_read_crossings_v1";
    let read_adapter = adapter_definitions_by_family
        .iter()
        .filter(|(family, _)| {
            qualification_function_is_imported(
                &current_sources["operation_read.rs"],
                family,
                read_adapter_name,
            )
        })
        .flat_map(|(_, definitions)| definitions.get(read_adapter_name).into_iter().flatten())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !read_adapter.is_empty(),
        "reopened-read adapter definition is attributable"
    );
    let read_evidence = evidence_for("operation_read.rs", read_name);
    let read_markers = [
        "FsCasV1::create_new",
        "FsCasV1::open_existing",
        "extract_root_v1",
        "read_file_range_impl_v1",
        "mutation_completed_while_read_blocked",
        "read_storage_terminal",
        "mutation_storage_terminal",
        "namespace_entries_are_regular",
    ];
    let missing_read_adapter_markers = read_markers[..4]
        .iter()
        .filter(|marker| !has_required(&read_adapter, std::slice::from_ref(marker)))
        .collect::<Vec<_>>();
    assert!(
        missing_read_adapter_markers.is_empty(),
        "reopened-read adapter is missing {missing_read_adapter_markers:?}"
    );
    let missing_read_markers = read_markers
        .iter()
        .filter(|marker| !has_required(&read_evidence, std::slice::from_ref(marker)))
        .collect::<Vec<_>>();
    assert!(
        missing_read_markers.is_empty(),
        "reopened-read evidence is missing {missing_read_markers:?}"
    );
    assert!(!has_required(
        &read_evidence.replace("FsCasV1::open_existing", "removed_reopen"),
        &read_markers
    ));
    assert!(!has_required(
        &read_evidence.replace(
            "mutation_completed_while_read_blocked",
            "removed_read_mutation_overlap"
        ),
        &read_markers
    ));

    let carrier_name = "carrier_cleanup_failure_invalidates_owner_and_root";
    let carrier_evidence = evidence_for("operation_faults.rs", carrier_name);
    let carrier_markers = [
        "owner_handle_invalidated",
        "stale_handle_invalidated",
        "stale_closure_refused",
        "reopen_invalidated",
        "PublicationErrorV1::TerminalFailure",
    ];
    assert!(has_required(&carrier_evidence, &carrier_markers));
    assert!(!has_required(
        &carrier_evidence.replace("stale_handle_invalidated", "removed_stale_proof"),
        &carrier_markers
    ));
    assert!(!has_required(
        &carrier_evidence.replace("reopen_invalidated", "removed_reopen_proof"),
        &carrier_markers
    ));
    assert!(!has_required(
        &carrier_evidence.replace("stale_closure_refused", "removed_stale_closure_proof"),
        &carrier_markers
    ));
    assert!(!has_required(
        &carrier_evidence.replace(
            "PublicationErrorV1::TerminalFailure",
            "PublicationErrorV1::CleanupFailed"
        ),
        &carrier_markers
    ));

    let move_name = "complete_cross_directory_move_detaches_and_attaches_in_one_handoff";
    let move_evidence = evidence_for("operation_mutation.rs", move_name);
    let move_markers = [
        "complete_cross_directory_move_operation_v1",
        "FsCasBoundaryV1::AfterCompleteValidatedHandoff",
        "validated_handoffs",
        "storage_terminals",
        "expected_roots_matched",
    ];
    assert!(has_required(&move_evidence, &move_markers));
    let move_body = normalized_semantic_source(function_body(
        current_sources["operation_mutation.rs"].as_str(),
        move_name,
    ));
    let exact_handoff_assertion = "assert_eq!(observation.validated_handoffs,1)";
    assert!(move_body.contains(exact_handoff_assertion));
    assert!(!move_body
        .replace(
            exact_handoff_assertion,
            "assert_eq!(observation.validated_handoffs,999)"
        )
        .contains(exact_handoff_assertion));
    assert!(!has_required(
        &move_evidence.replace(
            "FsCasBoundaryV1::AfterCompleteValidatedHandoff",
            "removed_validated_handoff"
        ),
        &move_markers
    ));

    let publication = evidence_for(
        "operation_concurrency.rs",
        "simultaneous_reopened_pack_callers_publish_one_canonical_shared_locator",
    );
    assert!(has_required(&publication, &["shared_id_matches"]));
    assert!(!has_required(
        &publication.replace("shared_id_matches", "removed_identity_proof"),
        &["shared_id_matches"]
    ));
    assert!(forbidden_final_owner_dispatch(
        "fn migrated() { qualification::run(ScenarioV1::new(7)); }"
    )
    .is_some());
    let faults = current_sources["operation_faults.rs"].as_str();
    assert!(support_helper_is_load_bearing(faults, "FaultPoint"));
    assert!(!support_helper_is_load_bearing(
        &faults.replace("&mut should_cancel", "|| true"),
        "FaultPoint"
    ));

    let subprocess_children = owner_names
        .iter()
        .map(|owner| {
            current_sources
                .get(owner)
                .unwrap()
                .matches("fn subprocess_open_existing_probe")
                .count()
        })
        .sum::<usize>();
    assert_eq!(
        subprocess_children, 1,
        "subprocess child custody is duplicated or absent"
    );
}

#[test]
fn pb08_default_object_reader_applicability_is_frozen() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/operation_read.rs"),
    )
    .expect("read operation_read owner");
    assert!(
        !source.contains("#[cfg(feature = \"operation-polymorphism\")]\nmod l1_object_read"),
        "default l1_object custody was gated behind operation-polymorphism"
    );
    let object_source = source
        .split_once("mod l1_object_read")
        .map(|(_, suffix)| suffix)
        .expect("l1_object_read owner exists");
    for name in [
        "all_five_exact_object_kinds_decode_and_hash_in_separate_domains",
        "bounded_random_read_decoder_matches_borrowed_decoder_and_never_requests_a_large_buffer",
        "hostile_envelopes_fail_before_visiting_edges",
        "hostile_payloads_abort_provisional_edges_and_reject_bad_order",
        "loop_counts_are_preflighted_against_declared_payload_bytes",
        "typed_edges_stream_in_wire_order_without_decoder_edge_storage",
    ] {
        assert_eq!(
            object_source.matches(&format!("fn {name}")).count(),
            1,
            "default object-reader custody missing or duplicated: {name}"
        );
    }
    assert_eq!(
        object_source.matches("#[test]").count(),
        6,
        "operation_read default object-reader owner lost exact discovery"
    );
}

#[test]
fn concrete_storage_modules_and_c3_grants_are_not_a_dependent_crate_sdk() {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
    let fixture = std::env::temp_dir().join(format!(
        "layerfs-l155-private-surface-{}-{sequence:016x}",
        std::process::id()
    ));
    let source_dir = fixture.join("src");
    let target_dir = fixture.join("target");
    fs::create_dir_all(&source_dir).expect("create compile-fail fixture");

    let dependency_path = manifest_dir
        .to_str()
        .expect("storage manifest path must be UTF-8");
    let manifest = |features: &str| {
        format!(
            "[package]\nname = \"layerfs-l155-private-surface-fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[dependencies]\nlayerfs-storage = {{ path = {dependency_path:?}{features} }}\n"
        )
    };
    let run_check = || {
        Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .arg("check")
            .arg("--offline")
            .arg("--quiet")
            .current_dir(&fixture)
            .env("CARGO_TARGET_DIR", &target_dir)
            .output()
            .expect("run dependent-crate qualification check")
    };

    fs::write(fixture.join("Cargo.toml"), manifest("")).expect("write feature-off manifest");
    fs::write(
        source_dir.join("main.rs"),
        "use layerfs_storage::qualification;\nfn main() { let _ = qualification::resources::base_ledger_bytes_v1(); }\n",
    )
    .expect("write feature-off source");
    let feature_off = run_check();
    assert!(
        !feature_off.status.success(),
        "feature-off qualification import compiled"
    );
    assert!(
        String::from_utf8_lossy(&feature_off.stderr).contains("qualification"),
        "feature-off failure did not name qualification"
    );

    fs::write(
        fixture.join("Cargo.toml"),
        manifest(", features = [\"operation-polymorphism\"]"),
    )
    .expect("write feature-on manifest");
    fs::write(
        source_dir.join("main.rs"),
        r#"use std::io::Cursor;
use std::path::Path;
use layerfs_storage::qualification::cas::semantic::{read_v1_to_writer, PublicationRequestV1, ReadObjectKindV1, ReadRequestV1};
use layerfs_storage::qualification::content::semantic::{create_v1, update_from_reader_v1, ContentRequestV1, UpdateRequestV1};
fn main() {
    let request = ContentRequestV1::new(b"approved", 0o644, b"bounded request");
    let observation = create_v1(&request).expect("approved semantic flow");
    let _ = observation.logical_len();
    let objects: [&[u8]; 0] = [];
    let _ = PublicationRequestV1::new(Path::new("."), &objects);
    let update = UpdateRequestV1::new(b"base", 0, 0, b"inserted");
    let mut source = Cursor::new(b"inserted");
    let _ = update_from_reader_v1(&update, 8, &mut source);
    let read = ReadRequestV1::new(ReadObjectKindV1::Chunk, b"bounded object");
    let mut sink = Vec::new();
    let _ = read_v1_to_writer(read, &mut sink);
}
"#,
    )
    .expect("write approved flow source");
    let approved = run_check();
    assert!(
        approved.status.success(),
        "approved qualification flow did not compile: {}",
        String::from_utf8_lossy(&approved.stderr)
    );

    fs::write(
        source_dir.join("main.rs"),
        r#"use layerfs_storage::identity::{ExplicitDirectoryNodeV1, ImplicitRootDirectoryV1};
fn main() {
    let _ = ExplicitDirectoryNodeV1::from_digest([0x11; 32]).id();
    let _ = ImplicitRootDirectoryV1::from_digest([0x22; 32]).id();
}
"#,
    )
    .expect("write raw-directory-identity source");
    let raw_directory_identities = run_check();
    assert!(
        !raw_directory_identities.status.success(),
        "dependent crate constructed raw directory identities"
    );
    let stderr = String::from_utf8_lossy(&raw_directory_identities.stderr);
    assert_eq!(
        stderr
            .matches("associated function `from_digest` is private")
            .count(),
        2,
        "dependent crate did not reject both raw directory wrappers: {stderr}"
    );

    fs::write(
        source_dir.join("main.rs"),
        r#"use layerfs_storage::cas::fs::FsCasV1;
use layerfs_storage::cow::tree::CanonicalDirectoryTreeV1;
use layerfs_storage::lifecycle::OperationHandoffV1;
use layerfs_storage::limits::ResourceLedgerV1;
use layerfs_storage::pack::CompletedPackSetV1;
use layerfs_storage::read::ReadSinkV1;
fn main() {}
"#,
    )
    .expect("write concrete-internal source");
    let internals = run_check();

    let _ = fs::remove_dir_all(&fixture);
    assert!(
        !internals.status.success(),
        "dependent crate unexpectedly compiled concrete L1.5.5 storage internals"
    );
    let stderr = String::from_utf8_lossy(&internals.stderr);
    for forbidden_module in ["cas", "cow", "lifecycle", "limits", "pack", "read"] {
        assert!(
            stderr.contains(&format!("module `{forbidden_module}` is private")),
            "dependent crate did not reject private {forbidden_module} internals: {stderr}"
        );
    }
}

#[test]
fn complete_content_depends_only_on_lifecycle_semantic_ports() {
    let create = include_str!("../src/content/create.rs");
    let production = production_source_v1(create);
    let lifecycle = include_str!("../src/lifecycle/mod.rs");
    for forbidden in [
        "crate::cas",
        "crate::cow",
        "crate::lifecycle",
        "crate::pack",
        "FsCas",
        "FsOperationSpool",
        "DirectPack",
        "FileChunkReferenceSpool",
        "FilePackIndexSpool",
        "FileClosureObjectSpool",
        "FileGlobalSeenSpool",
        "pack-index",
        "closure-objects",
        "global-seen",
        "private_pack",
        "CanonicalDirectoryTreeV1",
        "PreparedCandidateV1",
        "run_create_tree_v1",
        "run_create_v1",
    ] {
        assert!(
            !production.contains(forbidden),
            "content/create.rs retained complete-root/lifecycle ownership: {forbidden}"
        );
    }
    assert_eq!(production.matches("run_lifecycle_v1(").count(), 0);
    for operation in [
        "run_create_v1",
        "run_create_tree_v1",
        "run_complete_replace_v1",
        "run_complete_update_v1",
        "run_complete_add_v1",
        "run_complete_remove_v1",
        "run_complete_metadata_v1",
        "run_complete_move_v1",
        "complete_cross_directory_move_operation_v1",
    ] {
        let body = function_body(lifecycle, operation);
        assert!(
            body.contains("run_lifecycle_v1("),
            "{operation} does not enter the shared lifecycle coordinator"
        );
    }
    assert_eq!(
        lifecycle.matches("pub(crate) fn run_lifecycle_v1").count(),
        1,
        "complete operations must have one outer lifecycle state machine"
    );
    for duplicated_terminal in [
        "OperationPreparationV1",
        "begin_storage_session_v1",
        "complete_closure_fence_storage_v1",
        ".finish(control)",
    ] {
        assert!(
            !production.contains(duplicated_terminal),
            "content duplicated lifecycle terminal mechanics: {duplicated_terminal}"
        );
    }
}

#[test]
fn storage_mechanics_follow_semantic_module_ownership() {
    let cas = include_str!("../src/cas/mod.rs");
    let cas_fs = include_str!("../src/cas/fs.rs");
    let locator = include_str!("../src/cas/locator.rs");
    let locator_index = include_str!("../src/cas/locator_index.rs");
    let lifecycle = include_str!("../src/lifecycle/mod.rs");
    let pack = include_str!("../src/pack/mod.rs");
    let object = include_str!("../src/object/mod.rs");
    let read = include_str!("../src/read/mod.rs");
    let cow = include_str!("../src/cow/mod.rs");
    let content = include_str!("../src/content/mod.rs");

    for forbidden in ["mod c3_storage;", "mod operation_storage;"] {
        assert!(
            !cas.contains(forbidden),
            "CAS catch-all remains: {forbidden}"
        );
    }
    for owned in [
        "mod closure_storage;",
        "mod locator;",
        "mod locator_index;",
        "mod operation_admission;",
    ] {
        assert!(cas.contains(owned), "missing CAS-owned module: {owned}");
    }
    for owned in ["mod complete_writer;", "mod operation_index;"] {
        assert!(pack.contains(owned), "missing pack-owned module: {owned}");
    }
    assert!(
        cas_fs.contains("read_sealed_pack_shape_v1(&mut reader)"),
        "cas/fs.rs must delegate sealed-pack shape decoding to pack"
    );
    assert!(
        !cas_fs.contains("fn read_sealed_shape("),
        "cas/fs.rs still owns sealed-pack shape decoding"
    );
    for duplicated_pack_layout in ["header[48..52]", "header[56..64]", "len - 32"] {
        assert!(
            !cas_fs.contains(duplicated_pack_layout),
            "cas/fs.rs duplicated pack-owned layout: {duplicated_pack_layout}"
        );
    }
    let sealed_shape = function_body(pack, "read_sealed_pack_shape_v1");
    for required in [
        "PACK_HEADER_BYTES + PACK_TRAILER_BYTES",
        "pack.len().map_err(map_read_port)",
        "pack.read_exact_at(0, &mut header)",
        "be_u32(&header[48..52])",
        "be_u64(&header[56..64])",
        "checked_sub(DIGEST_BYTES as u64)",
        "SealedPackV1::from_validated_parts",
    ] {
        assert!(
            sealed_shape.contains(required),
            "pack sealed-shape decoder lacks owned semantics: {required}"
        );
    }
    for owned in [
        "mod model;",
        "mod encode;",
        "mod decode;",
        "mod port_decode;",
        "mod traversal;",
    ] {
        assert!(
            object.contains(owned),
            "missing object-owned module: {owned}"
        );
    }
    for owned in ["mod extraction;", "mod range;", "mod object_reader;"] {
        assert!(read.contains(owned), "missing read-owned module: {owned}");
    }
    for owned in ["mod file;", "mod tree;", "mod view;", "mod mutate;"] {
        assert!(cow.contains(owned), "missing COW-owned module: {owned}");
    }
    for owned in ["mod file;", "mod create;", "mod replace;", "mod update;"] {
        assert!(
            content.contains(owned),
            "missing content-owned module: {owned}"
        );
    }
    assert!(lifecycle.contains("mod preparation;"));
    assert!(lifecycle.contains("pub(crate) fn run_lifecycle_v1"));
    assert!(lifecycle.contains("preparation.finish(control)"));

    assert!(locator.contains("b\"LFSOBJ01\""));
    assert!(locator.contains("encode_persistent_locator_v1"));
    assert!(locator.contains("decode_persistent_locator_v1"));
    assert!(locator.contains("locator_transaction_tag_v1"));
    assert!(locator.contains("PersistentLocatorPublicationEvidenceV1"));
    assert!(locator.contains("PersistentLocatorPublicationDecisionV1"));
    assert!(locator.contains("decide_persistent_locator_publication_v1"));
    assert!(locator.contains("PersistentLocatorBindingEvidenceV1"));
    assert!(locator.contains("PersistentLocatorBindingDecisionV1"));
    assert!(locator.contains("decide_persistent_locator_binding_v1"));
    assert!(locator.contains("PersistentLocatorCatalogBindingDecisionV1"));
    assert!(locator.contains("decide_persistent_locator_catalog_binding_v1"));
    assert!(locator.contains("PersistentLocatorIncumbentEvidenceV1"));
    assert!(locator.contains("PersistentLocatorIncumbentDecisionV1"));
    assert!(locator.contains("decide_persistent_locator_incumbent_v1"));
    assert!(locator.contains("PersistentCatalogIncumbentDecisionV1"));
    assert!(locator.contains("decide_persistent_catalog_incumbent_v1"));
    for forbidden in ["std::fs", "OpenOptions", "hard_link", "lock_visibility"] {
        assert!(
            !locator.contains(forbidden),
            "locator policy module crossed into filesystem mechanics: {forbidden}"
        );
    }

    let publication_decision = function_body(locator, "decide_persistent_locator_publication_v1");
    for required in [
        "evidence.locator.transaction() == evidence.transaction",
        "evidence.locator.sealed() == evidence.sealed",
        "evidence.locator.entry() == evidence.entry",
        "PersistentLocatorPublicationDecisionV1::Authenticated",
        "PersistentLocatorPublicationDecisionV1::Foreign",
    ] {
        assert!(
            publication_decision.contains(required),
            "locator publication decision lacks substantive custody policy: {required}"
        );
    }
    let binding_decision = function_body(locator, "decide_persistent_locator_binding_v1");
    for required in [
        "locator.sealed == catalog",
        "locator.entry == indexed",
        "PersistentLocatorBindingDecisionV1::Authenticated",
        "PersistentLocatorBindingDecisionV1::Collision",
    ] {
        assert!(
            binding_decision.contains(required),
            "locator binding decision lacks substantive policy: {required}"
        );
    }
    let catalog_binding_decision =
        function_body(locator, "decide_persistent_locator_catalog_binding_v1");
    for required in [
        "locator.sealed == catalog",
        "PersistentLocatorCatalogBindingDecisionV1::Authenticated",
        "PersistentLocatorCatalogBindingDecisionV1::Collision",
    ] {
        assert!(
            catalog_binding_decision.contains(required),
            "catalog binding decision lacks substantive policy: {required}"
        );
    }
    let incumbent_decision = function_body(locator, "decide_persistent_locator_incumbent_v1");
    for required in [
        "decide_persistent_locator_binding_v1",
        "same_object_identity_v1",
        "evidence.object_bytes_equal",
        "PersistentLocatorIncumbentDecisionV1::EqualReuse",
        "PersistentLocatorIncumbentDecisionV1::BindingCollision",
        "PersistentLocatorIncumbentDecisionV1::UnequalObject",
    ] {
        assert!(
            incumbent_decision.contains(required),
            "locator incumbent decision lacks substantive policy: {required}"
        );
    }
    let object_identity = function_body(locator, "same_object_identity_v1");
    for required in [".id()", ".object_len()", ".object_checksum()", "&&"] {
        assert!(
            object_identity.contains(required),
            "locator object identity policy is too thin: {required}"
        );
    }
    let catalog_incumbent = function_body(locator, "decide_persistent_catalog_incumbent_v1");
    for required in [
        "incumbent.id() != expected.id()",
        "incumbent == expected",
        "PersistentCatalogIncumbentDecisionV1::Authenticated",
        "PersistentCatalogIncumbentDecisionV1::Collision",
        "PersistentCatalogIncumbentDecisionV1::Unequal",
    ] {
        assert!(
            catalog_incumbent.contains(required),
            "catalog incumbent decision lacks substantive policy: {required}"
        );
    }

    assert!(cas_fs.contains("gather_object_locator_incumbent_evidence"));
    assert!(cas_fs.contains("decide_persistent_locator_install_v1"));
    assert!(cas_fs.contains("PersistentLocatorInstallObservationV1::Incumbent"));
    assert!(cas_fs.contains("map_persistent_locator_install_decision_v1"));
    assert!(cas_fs.contains("PersistentLocatorIncumbentEvidenceV1::new"));
    let gather_incumbent = function_body(cas_fs, "gather_object_locator_incumbent_evidence");
    for required in [
        "open_occupant",
        "locate_validated_pack_index_entry_controlled_v1",
        "validate_validated_pack_object_controlled_v1",
        "compare_complete_object_bytes",
        "revalidate_immutable_file_snapshot_v1",
        "PersistentLocatorIncumbentEvidenceV1::new",
    ] {
        assert!(
            gather_incumbent.contains(required),
            "fs evidence gatherer does not own the required physical authentication step: {required}"
        );
    }
    for forbidden in [
        "same_object_identity_v1",
        "PersistentLocatorIncumbentDecisionV1::EqualReuse",
        "locator.sealed == catalog",
        "locator.entry == indexed",
    ] {
        assert!(
            !gather_incumbent.contains(forbidden),
            "fs evidence gatherer reimplemented locator policy: {forbidden}"
        );
    }
    let install_locators = function_body(cas_fs, "install_object_locators");
    for required in [
        "gather_object_locator_incumbent_evidence",
        "decode_persistent_locator_for_install_v1",
        "decide_persistent_locator_install_v1",
        "PersistentLocatorInstallObservationV1::Incumbent",
        "map_persistent_locator_install_decision_v1",
    ] {
        assert!(
            install_locators.contains(required),
            "fs installation path does not delegate locator meaning through the typed seam: {required}"
        );
    }
    assert!(!install_locators.contains("same_object_identity_v1"));
    for forbidden in [
        "decide_persistent_locator_incumbent_v1",
        "PersistentLocatorIncumbentDecisionV1::EqualReuse",
        "PersistentLocatorIncumbentDecisionV1::BindingCollision",
        "PersistentLocatorIncumbentDecisionV1::UnequalObject",
        "PersistentLocatorBindingDecisionV1::Authenticated",
        "PersistentLocatorBindingDecisionV1::Collision",
        "receipt.locator.transaction()",
        "decode_persistent_locator_v1",
        "map_persistent_locator_codec_error_v1",
    ] {
        assert!(
            !install_locators.contains(forbidden),
            "fs installation path still interprets locator policy directly: {forbidden}"
        );
    }
    let install_decision = function_body(locator, "decide_persistent_locator_install_v1");
    for required in [
        "decide_persistent_locator_incumbent_v1",
        "PersistentLocatorInstallDecisionV1::Installed",
        "PersistentLocatorInstallDecisionV1::EqualReuse",
        "PersistentLocatorInstallDecisionV1::BindingCollision",
        "PersistentLocatorInstallDecisionV1::UnequalObject",
    ] {
        assert!(
            install_decision.contains(required),
            "locator install decision lacks substantive policy: {required}"
        );
    }
    let decode_install = function_body(locator, "decode_persistent_locator_for_install_v1");
    for required in [
        "decode_persistent_locator_v1",
        "PersistentLocatorCodecErrorV1::Malformed",
        "PersistentLocatorCodecErrorV1::BindingMismatch",
        "PersistentLocatorInstallDecisionV1::Malformed",
        "PersistentLocatorInstallDecisionV1::BindingCollision",
    ] {
        assert!(
            decode_install.contains(required),
            "locator install decoder does not own codec classification: {required}"
        );
    }
    let decode_receipt = function_body(cas_fs, "decode_locator_publication_receipt_v1");
    assert!(
        decode_receipt.contains("decode_persistent_locator_self_describing_v1"),
        "receipt decoder must delegate persistent locator binding interpretation"
    );
    for forbidden in [
        "PhysicalObjectKindV1",
        "from_kind_and_digest",
        "locator_bytes[8]",
        "locator_bytes[16..48]",
    ] {
        assert!(
            !decode_receipt.contains(forbidden),
            "receipt decoder duplicated persistent locator layout: {forbidden}"
        );
    }
    let self_describing = function_body(locator, "decode_persistent_locator_self_describing_v1");
    for required in [
        "PhysicalObjectKindV1::try_from(bytes[8])",
        "bytes[16..48]",
        "TypedPhysicalObjectIdV1::from_kind_and_digest",
        "decode_persistent_locator_v1",
    ] {
        assert!(
            self_describing.contains(required),
            "locator self-describing decoder lacks owned binding interpretation: {required}"
        );
    }
    let map_install = function_body(cas_fs, "map_persistent_locator_install_decision_v1");
    for required in [
        "PersistentLocatorInstallDecisionV1::Installed",
        "PersistentLocatorInstallDecisionV1::EqualReuse",
        "PersistentLocatorInstallDecisionV1::BindingCollision",
        "PersistentLocatorInstallDecisionV1::UnequalObject",
        "map_persistent_locator_install_error_v1",
    ] {
        assert!(
            map_install.contains(required),
            "fs adapter does not map the full locator-owned install decision: {required}"
        );
    }
    let rollback = function_body(cas_fs, "rollback_unpublished_admission");
    for required in [
        "PersistentLocatorRollbackEvidenceV1::new",
        "decide_persistent_locator_rollback_v1",
        "PersistentLocatorRollbackDecisionV1::Authorized",
        "revalidate_immutable_file_snapshot_v1",
        "fs::remove_file(&path)",
    ] {
        assert!(
            rollback.contains(required),
            "rollback does not authenticate exact locator publication custody: {required}"
        );
    }
    for substantive_locator_policy in [
        "validate_and_compare_object_locator",
        "classify_persistent_locator_binding_v1",
        "persistent_locator_matches_catalog_v1",
        "persistent_locator_matches_index_entry_v1",
        "if locator.sealed == catalog",
        "if locator.entry == indexed",
        "if locator.entry == candidate_entry",
        "if !object_bytes_equal",
    ] {
        assert!(
            !cas_fs.contains(substantive_locator_policy),
            "cas/fs.rs still reimplements locator-owned policy: {substantive_locator_policy}"
        );
    }
    for duplicated_persistent_owner in [
        "const OBJECT_LOCATOR_MAGIC",
        "fn encode_object_locator",
        "fn decode_object_locator",
        "struct ObjectLocatorV1",
    ] {
        assert!(
            !cas_fs.contains(duplicated_persistent_owner),
            "cas/fs.rs still owns persistent locator policy: {duplicated_persistent_owner}"
        );
    }
    for forbidden_transient_authority in [
        "LFSOBJ01",
        "encode_persistent_locator",
        "decode_persistent_locator",
        "publish_small_marker",
        "catalog",
    ] {
        assert!(
            !locator_index.contains(forbidden_transient_authority),
            "transient locator index gained publication authority: {forbidden_transient_authority}"
        );
    }
}

#[test]
fn pb07_substantive_owners_are_not_forwarding_shells() {
    let tree = include_str!("../src/cow/tree.rs");
    let mutate = include_str!("../src/cow/mutate.rs");
    let view = include_str!("../src/cow/view.rs");
    assert!(view.contains("trait CanonicalTreeMutationSourceV1"));
    for view_owned_authentication in [
        "struct SparsePreimageHasherV1",
        "fn hash_streamed_directory_side",
        "fn validate_mutation_relation",
        "fn authenticate_and_derive_mutation_logical",
        "fn validate_mutation_physical_evidence",
        "fn validate_replacement_evidence",
        "fn derive_replacement_logical",
        "fn validate_page_boundaries",
    ] {
        assert!(
            view.contains(view_owned_authentication),
            "cow/view.rs lacks substantive authenticated-view ownership: {view_owned_authentication}"
        );
    }
    assert!(mutate.contains("CanonicalTreeMutationSourceV1"));
    assert!(mutate.contains("TreeProofMutationV1"));
    assert!(mutate.contains("fn mutate_directory_entries_cow_v1"));
    assert!(mutate.contains("fn mutate_directory_entries_inner"));
    assert!(mutate.contains("fn replace_directory_entry_cow_with_admission_v1"));
    assert!(!mutate.contains("tree::replace_directory_entry_cow_impl_v1"));
    assert!(!mutate.contains("tree::add_directory_entry_cow_impl_v1"));
    assert!(!mutate.contains("tree::remove_directory_entry_cow_impl_v1"));
    for tree_owned_mutation in [
        "fn replace_directory_entry_cow_impl_v1",
        "fn replace_directory_entry_cow_borrowed_impl_v1",
        "fn mutate_directory_entries_cow_v1",
        "fn mutate_directory_entries_inner",
        "enum TreeMutationKindV1",
    ] {
        assert!(
            !tree.contains(tree_owned_mutation),
            "COW mutation implementation remains in cow/tree.rs: {tree_owned_mutation}"
        );
    }
    for proof_owned_function in [
        "fn hash_streamed_directory_side",
        "fn validate_mutation_relation",
        "fn authenticate_and_derive_mutation_logical",
        "fn validate_mutation_physical_evidence",
        "fn validate_replacement_evidence",
        "struct SparsePreimageHasherV1",
        "struct VerificationTreeSinkV1",
    ] {
        assert!(
            !mutate.contains(proof_owned_function),
            "authenticated COW view implementation remains in cow/mutate.rs: {proof_owned_function}"
        );
    }

    let extraction = include_str!("../src/read/extraction.rs");
    let range = include_str!("../src/read/range.rs");
    let content_read = include_str!("../src/content/read.rs");
    for content_owned_streaming in [
        "struct VerifiedFileSegmentV1",
        "trait VerifiedFileRangePortV1",
        "fn stream_verified_file_range_v1",
    ] {
        assert!(
            content_read.contains(content_owned_streaming),
            "content/read.rs lacks verified payload ownership: {content_owned_streaming}"
        );
    }
    for range_forbidden_streaming in [
        "struct VerifiedFileSegmentV1",
        "trait VerifiedFileRangePortV1",
        "fn stream_verified_file_range_v1",
    ] {
        assert!(
            !range.contains(range_forbidden_streaming),
            "read/range.rs retained verified payload streaming: {range_forbidden_streaming}"
        );
    }
    assert!(range.contains("struct ExactRangePlanV1"));
    assert!(range.contains("fn begin_exact_range_digest_v1"));
    assert!(!range.contains("super::extraction"));
    assert!(!range.contains("run_read_v1"));
    assert!(!range.contains("ReaderV1"));
    assert!(!range.contains("FsCas"));
    for extraction_owned_range in ["fn read_file_range_impl_v1", "fn read_exact_range"] {
        assert!(
            extraction.contains(extraction_owned_range),
            "root/path exact-range orchestration is missing from read/extraction.rs: {extraction_owned_range}"
        );
    }
    assert!(extraction.contains("impl<C: FsCasControlV1 + ?Sized> VerifiedFileRangePortV1"));
    assert!(!extraction.contains("ExactRangeExecutorV1"));
    for raw_object_layout in [
        "ObjectCursorV1",
        "read_u8",
        "read_u16",
        "read_u32",
        "read_u64",
        "read_component",
        "ExtentTagV1",
        "PhysicalTreeChildKindV1",
        "TreeSubtypeV1",
        "OBJECT_HEADER_BYTES",
    ] {
        assert!(
            !range.contains(raw_object_layout),
            "read/range.rs retained raw object-layout parsing: {raw_object_layout}"
        );
        assert!(
            !extraction.contains(raw_object_layout),
            "read/extraction.rs retained raw object-layout parsing: {raw_object_layout}"
        );
    }

    let object_reader = include_str!("../src/read/object_reader.rs");
    for owned in [
        "struct OccupiedObjectReaderV1",
        "fn required_occupied_len_v1",
        "PhysicalObjectReadPortV1 for OccupiedObjectReaderV1",
    ] {
        assert!(
            object_reader.contains(owned),
            "object reader lacks bounded ownership: {owned}"
        );
    }
    assert!(!object_reader.contains("struct ObjectCursorV1"));
    assert!(!object_reader.contains("fn read_occupied_exact_accounted_v1"));
    assert!(!extraction.contains("struct ObjectCursorV1"));
    assert!(!extraction.contains("fn read_occupied_exact_accounted_v1"));

    let traversal = include_str!("../src/object/traversal.rs");
    let operation_admission = include_str!("../src/cas/operation_admission.rs");
    assert!(traversal.contains("fn traverse_strong_edges_v1"));
    assert!(traversal.contains("while"));
    assert!(operation_admission.contains("traverse_strong_edges_v1"));
    assert!(!operation_admission.contains("while ordinal < closure.count"));

    let resync = include_str!("../src/cdc/resync.rs");
    let update = include_str!("../src/content/update.rs");
    assert!(resync.contains("fn resynchronize_update_v1"));
    assert!(resync.contains("while base_cursor < base_len"));
    assert!(resync.contains("fn verify_rejoin_bytes_v1"));
    assert!(!update.contains("while base_cursor < base_len"));
    assert!(!update.contains("fn exact_rejoin_bytes"));
}

#[test]
fn pb07_canonical_transcripts_and_rejoin_proof_have_one_owner() {
    let encode = include_str!("../src/object/encode.rs");
    for owned in [
        "struct CanonicalPhysicalObjectEncoderV1",
        "struct CanonicalPhysicalObjectVerifierV1",
        "FramedHasherV1",
        "encode_physical_object_header_v1",
        "physical_domain_tag_v1",
        "FILE_FIXED_PAYLOAD_BYTES_V1",
        "DATA_EXTENT_FIXED_BYTES_V1",
        "CHUNK_REFERENCE_BYTES_V1",
    ] {
        assert!(
            encode.contains(owned),
            "object/encode.rs lacks canonical physical ownership: {owned}"
        );
    }

    let decode = include_str!("../src/object/decode.rs");
    let port_decode = include_str!("../src/object/port_decode.rs");
    assert!(decode.contains("trait CanonicalObjectCursorV1"));
    assert!(decode.contains("fn decode_payload_from_cursor_v1"));
    assert!(port_decode.contains("decode_payload_from_cursor_v1("));
    for duplicate in [
        "FramedHasherV1",
        "physical_domain_tag_v1",
        "fn decode_tree",
        "fn decode_file",
    ] {
        assert!(
            !port_decode.contains(duplicate),
            "object/port_decode.rs duplicated canonical semantic/transcript ownership: {duplicate}"
        );
    }

    for (path, source) in [
        ("content/file.rs", include_str!("../src/content/file.rs")),
        (
            "content/update.rs",
            include_str!("../src/content/update.rs"),
        ),
        ("cow/tree.rs", include_str!("../src/cow/tree.rs")),
    ] {
        let production = production_source_v1(source);
        for duplicate in [
            "FramedHasherV1",
            "TAG_PHYSICAL",
            "OBJECT_HEADER_BYTES",
            "FILE_FIXED_PAYLOAD_BYTES_V1",
            "DATA_EXTENT_FIXED_BYTES_V1",
            "CHUNK_REFERENCE_BYTES_V1",
            "encode_physical_object_header_v1",
            "physical_domain_tag_v1",
        ] {
            assert!(
                !production.contains(duplicate),
                "{path} retained canonical physical framing/transcript mechanics: {duplicate}"
            );
        }
    }

    assert!(encode.contains("struct EncodedVersionRecordV1"));
    assert!(encode.contains("fn encode_version_record_v1"));
    let complete_writer = include_str!("../src/pack/complete_writer.rs");
    assert!(complete_writer.contains("encode_version_record_v1("));
    for pack_forbidden_transcript in [
        "derive_physical_version_record_id_v1",
        "encode_physical_object_header_v1",
        "VERSION_RECORD_PAYLOAD_BYTES",
        "VERSION_OBJECT_BYTES",
        "payload[0..32]",
        "payload[176..184]",
    ] {
        assert!(
            !complete_writer.contains(pack_forbidden_transcript),
            "pack/complete_writer.rs retained VersionRecord layout/transcript ownership: {pack_forbidden_transcript}"
        );
    }

    let range = include_str!("../src/read/range.rs");
    assert!(!range.contains("crate::read::extraction"));
    assert!(!range.contains("super::extraction"));
    for forbidden in [
        "FsCas",
        "FsCasControlV1",
        "CanonicalDirectoryTreeV1",
        "read_object_payload_exact_v1",
        "PhysicalTreeChildKindV1",
        "RootDirectory",
    ] {
        assert!(
            !range.contains(forbidden),
            "read/range.rs crossed into extraction/storage ownership: {forbidden}"
        );
    }

    let resync = include_str!("../src/cdc/resync.rs");
    let update = include_str!("../src/content/update.rs");
    for owned in [
        "struct RejoinOperationBindingV1",
        "struct VerifiedRejoinV1",
        "fn verify_rejoin_bytes_v1",
        "fn consume",
    ] {
        assert!(
            resync.contains(owned),
            "cdc/resync.rs lacks authenticated rejoin ownership: {owned}"
        );
    }
    for duplicate in [
        "struct UpdateOperationBindingV1",
        "struct VerifiedRejoinV1",
        "fn verify_rejoin_bytes_v1",
        "ChunkerSpecV1",
    ] {
        assert!(
            !update.contains(duplicate),
            "content/update.rs retained CDC rejoin proof/profile ownership: {duplicate}"
        );
    }
}

#[test]
fn pb07_semantic_modules_keep_concrete_storage_out() {
    let sources = [
        ("content/mod.rs", include_str!("../src/content/mod.rs")),
        ("content/file.rs", include_str!("../src/content/file.rs")),
        (
            "content/create.rs",
            include_str!("../src/content/create.rs"),
        ),
        (
            "content/replace.rs",
            include_str!("../src/content/replace.rs"),
        ),
        (
            "content/update.rs",
            include_str!("../src/content/update.rs"),
        ),
        ("content/read.rs", include_str!("../src/content/read.rs")),
        ("read/range.rs", include_str!("../src/read/range.rs")),
        ("cow/file.rs", include_str!("../src/cow/file.rs")),
        ("cow/tree.rs", include_str!("../src/cow/tree.rs")),
        ("cow/view.rs", include_str!("../src/cow/view.rs")),
        ("cow/mutate.rs", include_str!("../src/cow/mutate.rs")),
        ("object/model.rs", include_str!("../src/object/model.rs")),
        ("object/encode.rs", include_str!("../src/object/encode.rs")),
        ("object/decode.rs", include_str!("../src/object/decode.rs")),
        (
            "object/port_decode.rs",
            include_str!("../src/object/port_decode.rs"),
        ),
        (
            "object/traversal.rs",
            include_str!("../src/object/traversal.rs"),
        ),
    ];
    for (path, source) in sources {
        let production = production_source_v1(source);
        if path.starts_with("content/") {
            assert_eq!(
                forbidden_content_import_v1(&production),
                None,
                "{path} crossed a concrete storage/import boundary"
            );
        }
        for forbidden in [
            "crate::cas::fs",
            "FsPrivatePack",
            "FsOperationSpool",
            "FileClosureObjectSpool",
            "FileGlobalSeenSpool",
            "hard_link",
        ] {
            assert!(
                !production.contains(forbidden),
                "{path} crossed a concrete storage/publication boundary: {forbidden}"
            );
        }
    }
}

#[test]
fn pb07_content_storage_import_guard_rejects_representative_forbidden_imports() {
    for source in [
        "use crate::cas::FsCasV1;",
        "use crate::pack::CompletedPackSetV1;",
        "use crate::lifecycle::run_lifecycle_v1;",
        "use std::fs::OpenOptions;",
        "use std::path::PathBuf;",
    ] {
        assert!(
            forbidden_content_import_v1(source).is_some(),
            "content import guard accepted {source}"
        );
    }
}

#[test]
fn pb07_private_migration_names_are_absent_from_production() {
    let sources = [
        include_str!("../src/lib.rs"),
        include_str!("../src/content/mod.rs"),
        include_str!("../src/content/file.rs"),
        include_str!("../src/content/create.rs"),
        include_str!("../src/content/replace.rs"),
        include_str!("../src/content/update.rs"),
        include_str!("../src/content/read.rs"),
        include_str!("../src/cow/mod.rs"),
        include_str!("../src/cow/file.rs"),
        include_str!("../src/cow/tree.rs"),
        include_str!("../src/cow/view.rs"),
        include_str!("../src/cow/mutate.rs"),
        include_str!("../src/object/mod.rs"),
        include_str!("../src/object/model.rs"),
        include_str!("../src/object/encode.rs"),
        include_str!("../src/object/decode.rs"),
        include_str!("../src/object/port_decode.rs"),
        include_str!("../src/object/traversal.rs"),
        include_str!("../src/read/mod.rs"),
        include_str!("../src/read/extraction.rs"),
        include_str!("../src/read/range.rs"),
        include_str!("../src/read/object_reader.rs"),
        include_str!("../src/lifecycle/mod.rs"),
        include_str!("../src/lifecycle/preparation.rs"),
    ];
    for source in sources {
        let source = production_source_v1(source);
        let lower = source.to_ascii_lowercase();
        for forbidden in [
            "run_c3_",
            "request_c3_",
            "c3_storage_",
            "c3_admission_",
            "c3operationpreparation",
            "sharedc3control",
        ] {
            assert!(
                !lower.contains(forbidden),
                "private migration name remains: {forbidden}"
            );
        }
    }
}

#[test]
fn historical_c3_source_is_immutable_and_not_a_current_target() {
    let manifest = include_str!("../Cargo.toml");
    let lib = include_str!("../src/lib.rs");
    let content = include_str!("../src/content/mod.rs");
    let cas = include_str!("../src/cas/mod.rs");
    let cow = include_str!("../src/cow/mod.rs");
    let create = include_str!("../src/content/create.rs");
    let historical_source = include_bytes!("../src/bin/c3_qualification.rs");
    let historical_fixture = include_bytes!("fixtures/c3-registry-v1.tsv");

    assert!(manifest.contains("autobins = false"));
    assert!(manifest.contains("autotests = false"));
    assert!(!manifest.contains("name = \"c3-qualification\""));
    assert_eq!(historical_source.len(), 49_821);
    let digest = support::sha256(historical_source)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        digest,
        "0f6f731e366a4802cac801ceacf8cb75d75296494f49c036ac368fcf31ca7da6"
    );
    assert_eq!(historical_fixture.len(), 47_759);
    let fixture_digest = support::sha256(historical_fixture)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        fixture_digest,
        "db8d1f2239cdbcfc3b37a050859533dea547b5d690dc17fd09099a0f6539ea61"
    );
    for module in ["cas", "content", "cow", "limits", "pack"] {
        assert!(lib.contains(&format!("pub(crate) mod {module};")));
        assert!(!lib.contains(&format!("pub mod {module};")));
    }
    assert!(!content.contains("pub use create::*"));
    assert!(!cas.contains("pub use port::*"));
    assert!(!cow.contains("pub use tree::*"));
    for leaked_surface in [
        "pub struct CreateOperationGrantV1",
        "pub fn request_create_operation_v1",
        "pub fn run_create_v1",
    ] {
        assert!(
            !create.contains(leaked_surface),
            "public C3 surface leaked: {leaked_surface}"
        );
    }
}
