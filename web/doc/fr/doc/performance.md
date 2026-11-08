# Performances

`fkv_bench` Testez les charges d'écriture, de lecture et d'analyse sur un espace clé fixe.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Les indicateurs incluent le débit, la latence à chaque étape et le pic RSS. version du workflow version x86_64 /
`fkv_bench` binaire pour aarch64.

## Match à trois avec RocksDB/fjall

`crate/perf/fkv_compare` Utilisez le même pilote de scène et le même encodage de clé (indice big-endian de 8 octets à la fin,
L'ordre du dictionnaire est cohérent avec l'ordre des indices) Test fkv, RocksDB, fjall : RocksDB réussi
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` La bibliothèque partagée compilée est chargée avec dlopen (le même C
ABI, la version Rust n'est pas liée à RocksDB), fjall utilise directement `fjall 3`. Chacun des trois moteurs ouvre une bibliothèque.
Avancez sur la même bibliothèque dans l'ordre des étapes : le chargement groupé crée des données, puis len/lecture de points/lecture de plage/
Les suppressions affectent toutes les mêmes données.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Ignorer la troisième colonne s'il n'y a pas de RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Les numéros de la Home Matrix ne peuvent provenir que d'une seule exécution traçable :

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI exécuté <id>"
```

`perf.yml` de GitHub Actions fait exactement cela (installer `librocksdb-dev`, programmer les adaptateurs, exécuter
Matrice de lignes de 25 M, téléchargez l'artefact et remplissez directement la page lorsque `backfill=true`).

### calibre

- **Rapport données/cache** : 25 000 000 de lignes × 116 B ≈ 2 766 Mo de données, 256 Mo de cache par moteur,
  ≈ 11:1, aligné sur l'examen officiel de RocksDB de 97 Gio de données/6 Gio de cache (≈ 16:1).
- **Même quantité de cache** : cache de pages de fkv (même pool d'index/données), cache de blocs de RocksDB
  (`cache_index_and_filter_blocks=true`, taille de bloc 4 Ko), le propre cache de blocs de fjall
  Les budgets sont tous définis sur le même nombre d'octets.
- **Calibre de persistance (la colonne de comparaison utilise la configuration par défaut de RocksDB, sans alignement)** : fkv 1×page de données + par lot
  1×manifest `fdatasync` (Isomorphe à SyncData + AppendManifest de C++ EloqStore,
  Ni l'un ni l'autre n'a d'interrupteur d'arrêt) ; la colonne RocksDB utilise la **configuration par défaut** (WAL activé, `WriteOptions.sync=false`,
  Pas de fsync séquentiel), fjall utilise également la valeur par défaut (`PersistMode::Buffer`). Donc la phase d'écriture (écriture unique /
  La différence entre les écritures par lots) est la **différence de force de persistance**, et non la différence d'implémentation, qui doit être indiquée sur la page avec le numéro.
  Si vous souhaitez analyser "qui est le plus rapide avec la même force de synchronisation", ajoutez `--align-fsync` (ouvrez RocksDB avec sync=true,
  fjall utilise SyncAll), cette option n'est pas utilisée pour les matrices publiées en externe.
- **Deux pilotes fkv** : `--fkv-driver store` (par défaut, directement connecté à un seul `Store` au sein du processus, et
  RocksDB / fjall sont aussi des appels de bibliothèque) et `--fkv-driver runtime` (mode de déploiement : un processus
  `--fkv-shards` threads de fragmentation, les requêtes sont acheminées par clé, alignées sur la lecture et l'écriture publiques de C++ EloqStore
  interface). Chaque requête de ce dernier doit passer par la file d'attente + réveil cross-thread, natif (macOS, le fichier IO passe par le pool de threads)
  Le temps de lecture est d'environ 20 µs/temps, ce qui est un ordre de grandeur plus lent qu'une connexion directe en cours de processus ; beaucoup moins cher sur Linux/io_uring, mais
  Encore un saut. La matrice de page utilise le calibre du magasin et le calibre d'exécution est utilisé pour "formulaire de déploiement vs C++
  Comparaison de "l'interface publique" (la lecture monothread du runtime fkv est d'environ 50 kops/s, C++ EloqStore FFI monothread
  fragment environ 21 kops/s).
- **Lire le calibre de concurrence** : numéro de thread côté fkv = `min(readers, --read-max-concurrency)`, par défaut
  La limite supérieure est de 4 (alignée sur le vCPU de la machine d'évaluation), **et non** "un thread par lecteur" - le niveau dépassé ne fait qu'augmenter la file d'attente ;
  Cache par thread = 256 Mio/nombre de threads (le montant total est fixe, non détruit par les lecteurs) et utilisé
  `FuturesUnordered` Conserver `--read-window` (16 par défaut) requêtes à la volée. Le côté RocksDB est
  `readers` Chaque `std::thread` lit simultanément ; boucle à filetage unique du côté fjall (en ignorant l'équipement des lecteurs,
  Comme colonne de référence uniquement). `--no-fkv-read-threads` peut dégénérer en "N chaînes asynchrones entrelacées sur une seule exécution".
  Il s'agit de la profondeur du pipeline, pas de la concurrence, et n'est pas comparable entre les moteurs.
- **len()** : aucun des trois moteurs n'a d'interface de comptage O(1). Cette ligne correspond au taux de comptage complet de la table, pas à la recherche O(1).