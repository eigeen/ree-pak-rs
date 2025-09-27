//! UTF-16 哈希优化基准测试

use criterion::{Criterion, criterion_group, criterion_main};
use ree_pak_core::utf16_hash::{Utf16HashExt, Utf16LeString};
use std::hint::black_box;

#[cfg(feature = "legacy-utf16-hash")]
use ree_pak_core::utf16_hash::legacy::FileNameFull;

/// 典型的长文件名（用于重点测试）
const TYPICAL_LONG_FILENAME: &str = "natives/stm/camera/collisionfilter/defaultcamera.cfil.7";

/// 基准测试：Mixed Hash性能对比 - 原始实现 vs 优化实现
fn bench_mixed_hash_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("mixed_hash_comparison");

    let filename = TYPICAL_LONG_FILENAME;

    // 原始实现
    #[cfg(feature = "legacy-utf16-hash")]
    group.bench_with_input("legacy", filename, |b, filename| {
        b.iter(|| {
            let original = FileNameFull::new(black_box(filename));
            black_box(original.hash_mixed())
        });
    });

    // 新优化实现（Utf16LeString）
    group.bench_with_input("optimized", filename, |b, filename| {
        b.iter(|| {
            let utf16_str = Utf16LeString::new_from_str(black_box(filename));
            black_box(utf16_str.hash_mixed())
        });
    });

    // FileNameExt trait实现 (字符串切片)
    group.bench_with_input("str_slice", filename, |b, filename| {
        b.iter(|| {
            let str_slice: &str = black_box(filename);
            black_box(str_slice.hash_mixed())
        });
    });

    group.finish();
}

/// 基准测试：Unicode字符处理性能
fn bench_unicode_handling(c: &mut Criterion) {
    let mut group = c.benchmark_group("unicode_handling");

    let unicode_files = &[
        "simple.txt",   // ASCII
        "测试中文.txt", // 中文
        "🦀emoji🔥.rs", // Emoji
        "Ñoño.file",    // 拉丁字符
        "Москва.dat",   // 西里尔字符
    ];

    for &filename in unicode_files {
        // 原始实现
        #[cfg(feature = "legacy-utf16-hash")]
        group.bench_with_input(format!("original/{}", filename), filename, |b, filename| {
            b.iter(|| {
                let original = FileNameFull::new(black_box(filename));
                black_box(original.hash_mixed())
            });
        });

        // 优化实现
        group.bench_with_input(format!("optimized/{}", filename), filename, |b, filename| {
            b.iter(|| {
                let utf16_str = Utf16LeString::new_from_str(black_box(filename));
                black_box(utf16_str.hash_mixed())
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_mixed_hash_comparison, bench_unicode_handling);

criterion_main!(benches);
