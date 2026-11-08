# Crate

| ชั้น | Crate | ความรับผิดชอบ |
|---|---|---|
| foundation | `fkv_types` | ID รูปแบบ ค่าคงที่ |
| foundation | `fkv_error` | โมเดลข้อผิดพลาด |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | การกำหนดค่าและการแยกวิเคราะห์ INI |
| foundation | `fkv_metrics` | ตัวนับและฮิสโตแกรม |
| foundation | `fkv_failpoint` | การฉีดความผิด |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | การบีบอัดดิบ / พจนานุกรม |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | จัดตำแหน่งบัฟเฟอร์ |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | มูลค่ามากและส่วน |
| io | `fkv_io` | ลักษณะ IO แบบอะซิงโครนัส |
| io | `fkv_io_compio` | แบ็กเอนด์คอมไพโอ |
| io | `fkv_io_mem` | แบ็กเอนด์หน่วยความจำ |
| io | `fkv_io_sim` | แบ็กเอนด์การจำลอง |
| io | `fkv_object` | ลักษณะการจัดเก็บอ็อบเจ็กต์ |
| io | `fkv_object_s3` | แบ็กเอนด์ S3 |
| io | `fkv_standby` | สำเนาสแตนด์บาย |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | ห่วงงานและการเข้าถึง |
| engine | `fkv_store` | วงจรชีวิตและการกำหนดเส้นทางของร้านค้า |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | เล่นซ้ำและเปิดใหม่ |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | เกณฑ์มาตรฐานแบบครบวงจร |