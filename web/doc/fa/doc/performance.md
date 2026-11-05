# عملکرد

`fkv_bench` بارهای نوشتن، خواندن و اسکن را روی یک فضای کلید ثابت آزمایش کنید.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

شاخص ها شامل توان عملیاتی، تأخیر در هر مرحله و پیک RSS است. انتشار جریان کار انتشار x86_64 /
`fkv_bench` باینری برای aarch64.