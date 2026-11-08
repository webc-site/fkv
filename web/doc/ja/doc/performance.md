# パフォーマンス

`fkv_bench` 固定キースペースで書き込み、読み取り、スキャンの負荷をテストします。

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

指標には、スループット、各ステージのレイテンシー、ピーク RSS が含まれます。リリース ワークフロー リリース x86_64 /
aarch64 の`fkv_bench`バイナリ。

## RocksDB/fjall との 3 者対戦

`crate/perf/fkv_compare` 同じステージ ドライバーと同じキー エンコーディングを使用します (末尾に 8 バイトのビッグエンディアンの添字、
辞書の順序は添字の順序と一致します）fkv、RocksDB、fjall のテスト: RocksDB が合格しました
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` コンパイルされた共有ライブラリは dlopen (同じ C
ABI、Rust ビルドは RocksDB にリンクしません)、fjall は`fjall 3`を直接使用します。 3 つのエンジンはそれぞれライブラリを開きます。
同じライブラリを段階の順序で進めます。バルクロードでデータを作成し、続いて len / ポイント読み取り / 範囲読み取り /
削除はすべて同じデータに影響します。

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # RocksDB がない場合は 3 列目をスキップします
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

ホーム マトリックスの数値は、追跡可能な 1 回の実行からのみ得られます。

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI 実行<id>"
```

GitHub Actions の `perf.yml` はまさにこれを行います (`librocksdb-dev` をインストールし、アダプターをプログラムし、実行します)
25M 行マトリックス、アーティファクトをアップロードし、`backfill=true`の場合にページを直接バックフィルします。

### 口径

- **データ/キャッシュ比**: 25,000,000 行 × 116 B ≈ 2766 MiB のデータ、エンジンあたり 256 MiB のキャッシュ、
  ≈ 11:1、97 GiB データ/6 GiB キャッシュ (≈ 16:1) の RocksDB の公式レビューと一致しています。
- **同じ量のキャッシュ**: fkv のページ キャッシュ (インデックス/データが同じプール)、RocksDB のブロック キャッシュ
  (`cache_index_and_filter_blocks=true`、ブロック サイズ 4 KiB)、fjall 独自のブロック キャッシュ
  バジェットはすべて同じバイト数に設定されます。
- **永続性の基準 (比較列はアライメントなしで RocksDB のデフォルト構成を使用します)**: fkv 1×データ ページ + バッチあたり
  1×マニフェスト `fdatasync` (C++ EloqStore の SyncData + AppendManifest と同型、
  どちらにもオフスイッチはありません）。 RocksDB 列は **デフォルト設定** (WAL オン、`WriteOptions.sync=false`、
  シーケンシャル fsync なし)、fjall もデフォルト (`PersistMode::Buffer`) を使用します。したがって、書き込みフェーズ (単一書き込み /
  バッチ書き込みの違いは、実装の違いではなく **永続性の強度の違い** であり、数値とともにページに記載する必要があります。
  「同じ同期強度で誰が速いか」を分析したい場合は、`--align-fsync` を追加します（RocksDB を sync=true で開き、
  fjall は SyncAll を使用します)、このオプションは外部に公開された行列には使用されません。
- **2 つの fkv ドライバー**: `--fkv-driver store` (デフォルト、プロセス内の単一の `Store` に直接接続されており、
  RocksDB / fjall もライブラリ呼び出しです) および `--fkv-driver runtime` (デプロイメント モード: プロセス)
  `--fkv-shards` シャード スレッド。リクエストはキーによってルーティングされ、C++ EloqStore のパブリック読み取りおよび書き込みと調整されます。
  インタフェース）。後者の各リクエストは、キュー + クロススレッド ウェイクアップ、ネイティブ (macOS、ファイル IO はスレッド プールを経由します) を経由する必要があります。
  読み取り時間は約 20 μs/回で、インプロセス直接接続よりも 1 桁遅くなります。 Linux/io_uring でははるかに安価ですが、
  まだジャンプです。ページ マトリックスではストア キャリバーが使用され、ランタイム キャリバーは「デプロイメント フォームと C++ の比較」に使用されます。
  「パブリック インターフェイス」の比較 (fkv ランタイム シングル スレッドの読み取りは約 50 kops/s、C++ EloqStore FFI シングル スレッド)
  シャードは約 21 kops/秒）。
- **読み取り同時実行能力**: fkv 側スレッド番号 = `min(readers, --read-max-concurrency)`、デフォルト
  上限は 4 (評価マシンの vCPU に合わせて) であり、**「リーダーごとに 1 スレッド」ではありません** - レベルを超えるとキューが増加するだけです。
  スレッドあたりのキャッシュ = 256 MiB / スレッド数 (合計量は固定であり、リーダーで細断されません)、使用されます
  `FuturesUnordered` オンザフライで `--read-window` (デフォルトは 16) リクエストを保持します。 RocksDB側は
  `readers` 各 `std::thread` は同時に読み取ります。 fjall 側のシングルスレッド ループ (リーダーのギアを無視し、
  参考コラムとしてのみ)。 `--no-fkv-read-threads` は、「単一のランタイム上のインターリーブされた N 個の非同期チェーン」に戻る可能性があります。
  これは同時実行性ではなくパイプラインの深さであり、エンジン間で比較することはできません。
- **len()**: 3 つのエンジンにはいずれも O(1) カウント インターフェイスがありません。この行は、O(1) ルックアップではなく、テーブル全体のカウント率です。