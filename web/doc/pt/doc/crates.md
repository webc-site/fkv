# Crate

| camada | Crate | Responsabilidades |
|---|---|---|
| foundation | `fkv_types` | ID, padrão, constante |
| foundation | `fkv_error` | modelo de erro |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Configuração e análise INI |
| foundation | `fkv_metrics` | Contadores e Histogramas |
| foundation | `fkv_failpoint` | injeção de falha |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | compactação bruta/de dicionário |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | alinhar buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Grande valor e segmento |
| io | `fkv_io` | Características de IO assíncronas |
| io | `fkv_io_compio` | back-end da compilação |
| io | `fkv_io_mem` | Back-end de memória |
| io | `fkv_io_sim` | Back-end de simulação |
| io | `fkv_object` | Características de armazenamento de objetos |
| io | `fkv_object_s3` | Back-end S3 |
| io | `fkv_standby` | cópia em espera |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | ciclo de trabalho e acesso |
| engine | `fkv_store` | armazenar ciclo de vida e roteamento |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | reproduzir e reabrir |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | referência de ponta a ponta |