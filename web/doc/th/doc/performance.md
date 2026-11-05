# ประสิทธิภาพ

`fkv_bench` ทดสอบการเขียน อ่าน และสแกนโหลดบนพื้นที่คีย์คงที่

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

ตัวบ่งชี้ประกอบด้วยปริมาณงาน เวลาแฝงในแต่ละขั้นตอน และ RSS สูงสุด ปล่อยเวิร์กโฟลว์ปล่อย x86_64 /
`fkv_bench` ไบนารีสำหรับ aarch64