/// 函数 `find_subsequence`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - haystack: 参数 haystack
/// - needle: 参数 needle
/// - start: 参数 start
///
/// # 返回
/// 返回函数执行结果
fn find_subsequence(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    if needle.is_empty() || start >= haystack.len() || haystack.len() < needle.len() {
        return None;
    }
    haystack[start..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|idx| idx + start)
}

/// 函数 `extract_multipart_part_name`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - headers: 参数 headers
///
/// # 返回
/// 返回函数执行结果
fn extract_multipart_part_name(headers: &[u8]) -> Option<String> {
    let headers_str = std::str::from_utf8(headers).ok()?;
    for line in headers_str.split("\r\n") {
        let (name, value) = line.split_once(':')?;
        if !name.trim().eq_ignore_ascii_case("content-disposition") {
            continue;
        }
        for token in value.split(';') {
            let token = token.trim();
            if token
                .get(..5)
                .map(|prefix| prefix.eq_ignore_ascii_case("name="))
                .unwrap_or(false)
            {
                let mut field_name = token[5..].trim().to_string();
                if field_name.starts_with('"') && field_name.ends_with('"') && field_name.len() >= 2
                {
                    field_name = field_name[1..field_name.len() - 1].to_string();
                }
                if !field_name.is_empty() {
                    return Some(field_name);
                }
            }
        }
    }
    None
}

/// 函数 `filter_multipart_form_data_body`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - path: 参数 path
/// - body: 参数 body
/// - retain_fn: 参数 retain_fn
///
/// # 返回
/// 返回函数执行结果
pub(super) fn filter_multipart_form_data_body(
    path: &str,
    body: &[u8],
    retain_fn: super::RetainFn,
) -> Option<(Vec<u8>, Vec<String>)> {
    if !body.starts_with(b"--") {
        return None;
    }
    let boundary_line_end = find_subsequence(body, b"\r\n", 0)?;
    if boundary_line_end <= 2 {
        return None;
    }
    let boundary = &body[2..boundary_line_end];
    if boundary.is_empty() {
        return None;
    }
    let mut boundary_marker = Vec::with_capacity(boundary.len() + 2);
    boundary_marker.extend_from_slice(b"--");
    boundary_marker.extend_from_slice(boundary);
    if !body.starts_with(&boundary_marker) {
        return None;
    }

    let mut delimiter_with_crlf = Vec::with_capacity(boundary_marker.len() + 2);
    delimiter_with_crlf.extend_from_slice(b"\r\n");
    delimiter_with_crlf.extend_from_slice(&boundary_marker);

    let mut cursor = boundary_marker.len();
    if body.get(cursor..cursor + 2) == Some(b"--") {
        return None;
    }
    if body.get(cursor..cursor + 2) != Some(b"\r\n") {
        return None;
    }
    cursor += 2;

    let mut kept_parts: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let mut dropped_keys = Vec::new();

    loop {
        let headers_end = find_subsequence(body, b"\r\n\r\n", cursor)?;
        let headers = &body[cursor..headers_end];
        let part_body_start = headers_end + 4;
        let next_boundary = find_subsequence(body, &delimiter_with_crlf, part_body_start)?;
        let part_body = &body[part_body_start..next_boundary];

        let keep = match extract_multipart_part_name(headers) {
            Some(name) => {
                if super::is_allowed_field(path, &name, retain_fn) {
                    true
                } else {
                    dropped_keys.push(name);
                    false
                }
            }
            None => true,
        };
        if keep {
            kept_parts.push((headers.to_vec(), part_body.to_vec()));
        }

        cursor = next_boundary + delimiter_with_crlf.len();
        if body.get(cursor..cursor + 2) == Some(b"--") {
            break;
        }
        if body.get(cursor..cursor + 2) != Some(b"\r\n") {
            return None;
        }
        cursor += 2;
    }

    if dropped_keys.is_empty() {
        return None;
    }

    let mut rebuilt = Vec::new();
    for (idx, (headers, part_body)) in kept_parts.iter().enumerate() {
        if idx > 0 {
            rebuilt.extend_from_slice(b"\r\n");
        }
        rebuilt.extend_from_slice(&boundary_marker);
        rebuilt.extend_from_slice(b"\r\n");
        rebuilt.extend_from_slice(headers);
        rebuilt.extend_from_slice(b"\r\n\r\n");
        rebuilt.extend_from_slice(part_body);
    }
    if !kept_parts.is_empty() {
        rebuilt.extend_from_slice(b"\r\n");
    }
    rebuilt.extend_from_slice(&boundary_marker);
    rebuilt.extend_from_slice(b"--\r\n");

    Some((rebuilt, dropped_keys))
}
