# أداء

`fkv_bench` اختبار عمليات الكتابة والقراءة والمسح الضوئي على مساحة مفتاح ثابتة.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

تشمل المؤشرات الإنتاجية وزمن الوصول في كل مرحلة وذروة RSS. إصدار سير العمل x86_64 /
`fkv_bench` ثنائي لـ aarch64.