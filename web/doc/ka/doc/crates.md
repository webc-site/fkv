# Crate

| ფენა | Crate | პასუხისმგებლობები |
|---|---|---|
| foundation | `fkv_types` | ID, ნიმუში, მუდმივი |
| foundation | `fkv_error` | შეცდომის მოდელი |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | კონფიგურაცია და INI ანალიზი |
| foundation | `fkv_metrics` | მრიცხველები და ჰისტოგრამები |
| foundation | `fkv_failpoint` | შეცდომის ინექცია |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | ნედლეული / ლექსიკონის შეკუმშვა |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | ბუფერის გასწორება |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | დიდი მნიშვნელობა და სეგმენტი |
| io | `fkv_io` | ასინქრონული IO თვისებები |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | მეხსიერების უკანა ნაწილი |
| io | `fkv_io_sim` | სიმულაციური ბექენდი |
| io | `fkv_object` | ობიექტების შენახვის თვისებები |
| io | `fkv_object_s3` | S3 backend |
| io | `fkv_standby` | ლოდინის ასლი |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | სამუშაო ციკლი და წვდომა |
| engine | `fkv_store` | შენახვის სასიცოცხლო ციკლი და მარშრუტირება |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | გამეორება და ხელახლა გახსნა |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | ბოლო-ბოლო საორიენტაციო ნიშანი |