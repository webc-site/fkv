# Crate

| పొర | Crate | బాధ్యతలు |
|---|---|---|
| foundation | `fkv_types` | ID, నమూనా, స్థిరం |
| foundation | `fkv_error` | లోపం నమూనా |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | కాన్ఫిగరేషన్ మరియు INI పార్సింగ్ |
| foundation | `fkv_metrics` | కౌంటర్లు మరియు హిస్టోగ్రాంలు |
| foundation | `fkv_failpoint` | తప్పు ఇంజెక్షన్ |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | ముడి / నిఘంటువు కుదింపు |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | బఫర్‌ని సమలేఖనం చేయండి |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | పెద్ద విలువ మరియు విభాగం |
| io | `fkv_io` | అసమకాలిక IO లక్షణాలు |
| io | `fkv_io_compio` | కంపియో బ్యాకెండ్ |
| io | `fkv_io_mem` | మెమరీ బ్యాకెండ్ |
| io | `fkv_io_sim` | అనుకరణ బ్యాకెండ్ |
| io | `fkv_object` | వస్తువు నిల్వ లక్షణాలు |
| io | `fkv_object_s3` | S3 బ్యాకెండ్ |
| io | `fkv_standby` | స్టాండ్బై కాపీ |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | పని లూప్ మరియు యాక్సెస్ |
| engine | `fkv_store` | నిల్వ జీవిత చక్రం మరియు రూటింగ్ |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | రీప్లే చేసి మళ్లీ తెరవండి |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | ఎండ్-టు-ఎండ్ బెంచ్‌మార్క్ |