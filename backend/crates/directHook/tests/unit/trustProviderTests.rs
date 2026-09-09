use super::*;

// 精确命中单个 Codex CA 变量，大小写按 Win32 规则；同前缀、其他变量和空指针均不接入。
#[test]
fn onlyExactCaVariableIsSelected() {
    for (key, expected) in [
        ("CODEX_CA_CERTIFICATE", true),
        ("codex_ca_certificate", true),
        ("CODEX_CA_CERTIFICATE_EXTRA", false),
        ("CODEX_CA", false),
        ("HTTP_PROXY", false),
        ("SSL_CERT_FILE", false),
        ("", false),
    ] {
        let wide: Vec<u16> = key.encode_utf16().chain([0]).collect();
        assert_eq!(unsafe { matchesCaVariable(wide.as_ptr()) }, expected);
    }
    assert!(!unsafe { matchesCaVariable(std::ptr::null()) });
}

// 长度查询、容量差一、恰好足够和超额缓冲都保留边界哨兵，覆盖 UTF-16 非 ASCII 路径。
#[test]
fn unicodeBufferContractDoesNotOverwriteBoundaries() {
    let path: Vec<u16> = "D:\\证书\\公开.pem".encode_utf16().chain([0]).collect();
    let required = path.len() as u32;
    assert_eq!(
        unsafe { copyBundlePath(&path, std::ptr::null_mut(), 0) },
        (required, false)
    );
    for capacity in [required - 1, required, required + 1] {
        let mut output = vec![0x7777; required as usize + 3];
        let (length, copied) =
            unsafe { copyBundlePath(&path, output.as_mut_ptr().add(1), capacity) };
        if capacity < required {
            assert_eq!((length, copied), (required, false));
            assert!(output.iter().all(|&value| value == 0x7777));
        } else {
            assert_eq!((length, copied), (required - 1, true));
            assert_eq!(&output[1..1 + path.len()], &path);
            assert_eq!(output[0], 0x7777);
            assert!(output[1 + path.len()..]
                .iter()
                .all(|&value| value == 0x7777));
        }
    }
}

// 只创建未启用的 trampoline，直接测试回调的非 CA 分支；不改系统入口或进程环境。
#[test]
fn unrelatedVariableKeepsOriginalReturnAndLastError() {
    unsafe {
        let module = GetModuleHandleW(w!("kernel32.dll")).unwrap();
        let address = GetProcAddress(module, s!("GetEnvironmentVariableW")).unwrap();
        originalVariable
            .prepareForTest(address as *const (), readVariable as *const ())
            .unwrap();
        let original: GetVariable = originalVariable.original();
        let key = w!("SystemRoot");
        let mut expected = [0u16; 512];
        let mut actual = [0u16; 512];
        let sentinel = windows::Win32::Foundation::WIN32_ERROR(7777);
        SetLastError(sentinel);
        let expectedLength = original(key.as_ptr(), expected.as_mut_ptr(), expected.len() as u32);
        let expectedError = GetLastError();
        SetLastError(sentinel);
        let actualLength = readVariable(key.as_ptr(), actual.as_mut_ptr(), actual.len() as u32);
        assert_eq!(GetLastError(), expectedError);
        assert_eq!(actualLength, expectedLength);
        assert_eq!(actual, expected);
        originalVariable.release().unwrap();
    }
}
