# Crate

| στρώμα | Crate | Ευθύνες |
|---|---|---|
| foundation | `fkv_types` | Ταυτότητα, μοτίβο, σταθερά |
| foundation | `fkv_error` | μοντέλο σφάλματος |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Διαμόρφωση και ανάλυση INI |
| foundation | `fkv_metrics` | Μετρητές και Ιστογράμματα |
| foundation | `fkv_failpoint` | έγχυση σφάλματος |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | ακατέργαστη / συμπίεση λεξικού |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | ευθυγράμμιση buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Μεγάλη τιμή και τμήμα |
| io | `fkv_io` | Ασύγχρονα χαρακτηριστικά IO |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | Backend μνήμης |
| io | `fkv_io_sim` | Backend προσομοίωσης |
| io | `fkv_object` | Χαρακτηριστικά αποθήκευσης αντικειμένων |
| io | `fkv_object_s3` | S3 backend |
| io | `fkv_standby` | αντίγραφο σε κατάσταση αναμονής |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | βρόχος εργασίας και πρόσβαση |
| engine | `fkv_store` | κύκλος ζωής και δρομολόγηση αποθήκευσης |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | επανάληψη και ανοίξτε ξανά |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | σημείο αναφοράς από άκρο σε άκρο |