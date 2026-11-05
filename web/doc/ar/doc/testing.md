# امتحان

## محلي

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` حالة استخدام السجل العالمي المشترك؛ أضف عند استخدام `cargo test` مباشرةً
<ب معرف = "1">.

## حاويات لينكس

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

يستخدم البرنامج النصي zigbuild لتجميع ثنائي اختبار Linux، ثم يضعه في الحد الأدنى من صورة دبيان للتشغيل.

## CI

`ci.yml` قم بتشغيل اختبارات FMT وClippy والاختبارات الكاملة واختبارات تكامل RustFS S3.
`linux-test.yml` تم إرجاعه ليلاً في Ubuntu 24.04 LTS وتم تأكيد 4 صفحات KiB.