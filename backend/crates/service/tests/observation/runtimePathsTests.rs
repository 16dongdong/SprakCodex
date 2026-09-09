use super::*;
use pelite::pe64::{Pe, PeFile};

// 构建脚本产物必须直接进入当前测试 EXE，并保持可解析的 x64 DLL 头。
#[test]
fn embeddedModuleIsLinkedIntoHostImage() {
    #[cfg(windows)]
    {
        let image = moduleImage().expect("Windows 构建必须包含内存载荷");
        let aligned = super::super::nativeInjection::alignedImage(image);
        let bytes =
            unsafe { std::slice::from_raw_parts(aligned.as_ptr().cast::<u8>(), image.len()) };
        let file = PeFile::from_bytes(bytes).expect("内嵌载荷必须是 PE32+");
        assert_eq!(
            file.file_header().Machine,
            pelite::pe64::image::IMAGE_FILE_MACHINE_AMD64
        );
        assert_ne!(
            file.file_header().Characteristics & pelite::pe64::image::IMAGE_FILE_DLL,
            0
        );
    }
}

// 公开证书仍按原子文件协议写入业务数据目录；失败不得遗留 pending 文件。
#[test]
fn failedAtomicPublishRemovesStagingFile() {
    let directory =
        std::env::temp_dir().join(format!("observationPaths{:032x}", rand::random::<u128>()));
    std::fs::create_dir(&directory).unwrap();
    let target = directory.join("blocked");
    std::fs::create_dir(&target).unwrap();
    assert!(writeAtomically(&target, b"fixture").is_err());
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
    std::fs::remove_dir(&target).unwrap();
    std::fs::remove_dir(&directory).unwrap();
}
