# Desempenho

`fkv_bench` Teste cargas de gravação, leitura e varredura em um espaço de chave fixa.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Os indicadores incluem rendimento, latência em cada estágio e pico de RSS. liberar liberação de fluxo de trabalho x86_64 /
`fkv_bench` binário para aarch64.

## Combinação de três vias com RocksDB/fjall

`crate/perf/fkv_compare` Use o mesmo driver de estágio e a mesma codificação de chave (subscrito big-endian de 8 bytes no final,
A ordem do dicionário é consistente com a ordem dos subscritos) Teste fkv, RocksDB, fjall: RocksDB aprovado
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` A biblioteca compartilhada compilada é carregada com dlopen (o mesmo C
ABI, Rust build não está vinculado ao RocksDB), fjall usa `fjall 3` diretamente. Cada um dos três motores abre uma biblioteca.
Avance na mesma biblioteca na ordem dos estágios: carregamento em massa cria dados e, em seguida, leitura de len/ponto/leitura de intervalo/
Todas as exclusões afetam os mesmos dados.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Pule a terceira coluna se não houver RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Os números da matriz da página inicial só podem vir de uma execução rastreável:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI executado <id>"
```

O `perf.yml` do GitHub Actions faz exatamente isso (instalar `librocksdb-dev`, programar adaptadores, executar
Matriz de linhas de 25 milhões, upload de artefato e preenchimento direto da página quando `backfill=true`).

### calibre

- **Proporção dados/cache**: 25.000.000 linhas × 116 B ≈ 2.766 MiB de dados, 256 MiB de cache por mecanismo,
  ≈ 11:1, alinhado com a análise oficial do RocksDB de dados de 97 GiB/cache de 6 GiB (≈ 16:1).
- **Mesma quantidade de cache**: cache de página do fkv (mesmo pool de índice/dados), cache de bloco do RocksDB
  (`cache_index_and_filter_blocks=true`, tamanho do bloco 4 KiB), cache de bloco do próprio fjall
  Os orçamentos são todos definidos com o mesmo número de bytes.
- **Calibre de persistência (a coluna de comparação usa a configuração padrão do RocksDB, sem alinhamento)**: fkv 1×página de dados + por lote
  1×manifest `fdatasync` (Isomórfico a SyncData + AppendManifest de C++ EloqStore,
  Nenhum deles tem botão para desligar); a coluna RocksDB usa a **configuração padrão** (WAL on, `WriteOptions.sync=false`,
  Sem fsync sequencial), fjall também usa o padrão (`PersistMode::Buffer`). Portanto, a fase de gravação (gravação única/
  A diferença entre gravações em lote) é a **diferença de força de persistência**, não a diferença de implementação, que deve ser indicada na página junto com o número.
  Se você quiser analisar "quem é mais rápido sob a mesma força de sincronização", adicione `--align-fsync` (abra RocksDB com sync=true,
  fjall usa SyncAll), esta opção não é usada para matrizes publicadas externamente.
- **Dois drivers fkv**: `--fkv-driver store` (padrão, conectado diretamente a um único `Store` dentro do processo, e
  RocksDB / fjall também são chamadas de biblioteca) e `--fkv-driver runtime` (modo de implantação: um processo
  `--fkv-shards` shard threads, as solicitações são roteadas por chave, alinhadas com a leitura e escrita pública do C++ EloqStore
  interface). Cada solicitação deste último precisa passar pela fila + ativação de thread cruzado, nativo (macOS, arquivo IO passa pelo pool de threads)
  O tempo de leitura é de cerca de 20 µs/tempo, o que é uma ordem de grandeza mais lenta que a conexão direta em processo; muito mais barato no Linux/io_uring, mas
  Ainda um salto. A matriz de página usa o calibre de armazenamento e o calibre de tempo de execução é usado para "formulário de implantação vs C++
  Comparação de "Interface Pública" (a leitura de thread único em tempo de execução fkv é de cerca de 50 kops/s, C++ EloqStore FFI de thread único
  fragmento cerca de 21 kops/s).
- **Calibre de simultaneidade de leitura**: número do thread lateral fkv = `min(readers, --read-max-concurrency)`, padrão
  O limite superior é 4 (alinhado com a vCPU da máquina de avaliação), **não** "um thread por leitor" - o nível excedido apenas aumenta a fila;
  Cache por thread = 256 MiB/número de threads (a quantidade total é fixa, não fragmentada com leitores) e usada
  `FuturesUnordered` Mantenha `--read-window` (padrão 16) solicitações em tempo real. O lado RocksDB é
  `readers` Cada `std::thread` lê simultaneamente; loop de thread único no lado fjall (ignorando o equipamento dos leitores,
  Apenas como coluna de referência). `--no-fkv-read-threads` pode degenerar novamente para "N cadeias assíncronas intercaladas em um único tempo de execução".
  Essa é a profundidade do pipeline, não a simultaneidade, e não é comparável entre mecanismos.
- **len()**: Nenhum dos três mecanismos possui interfaces de contagem O(1). Esta linha é a taxa de contagem completa da tabela, não a pesquisa O(1).