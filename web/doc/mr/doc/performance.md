# कामगिरी

`fkv_bench` निश्चित की स्पेसवर चाचणी लिहा, वाचा आणि लोड स्कॅन करा.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

निर्देशकांमध्ये थ्रूपुट, प्रत्येक टप्प्यावर विलंबता आणि शिखर RSS समाविष्ट आहे. रिलीझ वर्कफ्लो रिलीज x86_64 /
aarch64 साठी `fkv_bench` बायनरी.