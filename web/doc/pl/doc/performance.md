# Wydajność

`fkv_bench` Testuj obciążenia zapisu, odczytu i skanowania w stałej przestrzeni kluczy.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Wskaźniki obejmują przepustowość, opóźnienie na każdym etapie i szczyt RSS. wydanie przepływu pracy wydanie x86_64 /
`fkv_bench` plik binarny dla aarch64.

## Trójstronny pojedynek z RocksDB/fjall

`crate/perf/fkv_compare` Użyj tego samego sterownika scenicznego i tego samego kodowania klucza (8-bajtowy indeks dolny big-endian na końcu,
Kolejność słownikowa jest zgodna z kolejnością indeksu dolnego) Test fkv, RocksDB, fjall: RocksDB zaliczony
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Skompilowana biblioteka współdzielona jest ładowana za pomocą dlopen (tego samego C
ABI, kompilacja Rusta nie łączy się z RocksDB), fjall używa bezpośrednio `fjall 3`. Każdy z trzech silników otwiera bibliotekę.
Postępuj w tej samej bibliotece w kolejności etapów: ładowanie zbiorcze tworzy dane, a następnie len / odczyt punktu / odczyt zasięgu /
Wszystkie usunięcia dotyczą tych samych danych.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Pomiń trzecią kolumnę, jeśli nie ma RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Liczby matrycy strony głównej mogą pochodzić tylko z jednego identyfikowalnego przebiegu:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "Uruchomienie CI <id>"
```

`perf.yml` GitHub Actions robi dokładnie to (zainstaluj `librocksdb-dev`, adaptery programów, uruchom
Macierz 25 mln wierszy, prześlij artefakt i bezpośrednio uzupełnij stronę, gdy `backfill=true`).

### kaliber

- **Stosunek danych do pamięci podręcznej**: 25 000 000 wierszy × 116 B ≈ 2766 MiB danych, 256 MiB pamięci podręcznej na silnik,
  ≈ 11:1, zgodnie z oficjalną recenzją RocksDB dotyczącą 97 GiB danych/6 GiB pamięci podręcznej (≈ 16:1).
- **Ta sama ilość pamięci podręcznej**: pamięć podręczna stron fkv (indeks/data ta sama pula), pamięć podręczna bloków RocksDB
  (`cache_index_and_filter_blocks=true`, rozmiar bloku 4 KiB), własna pamięć podręczna bloków fjall
  Wszystkie budżety są ustawione na tę samą liczbę bajtów.
- **Kaliber trwałości (kolumna porównawcza wykorzystuje domyślną konfigurację RocksDB, bez wyrównania)**: fkv 1×strona danych + na partię
  1×manifest `fdatasync` (Izomorficzny do SyncData + AppendManifest C++ EloqStore,
  Żaden z nich nie ma wyłącznika); kolumna RocksDB używa **konfiguracji domyślnej** (WAL włączony, `WriteOptions.sync=false`,
  Brak sekwencyjnego fsync), fjall również używa ustawienia domyślnego (`PersistMode::Buffer`). Dlatego faza zapisu (pojedynczy zapis/
  Różnica między zapisami wsadowymi) to **różnica w sile trwałości**, a nie różnica w implementacji, którą należy podać na stronie wraz z liczbą.
  Jeśli chcesz sprawdzić, kto jest szybszy przy tej samej sile synchronizacji, dodaj `--align-fsync` (otwórz RocksDB z sync=true,
  fjall używa SyncAll), ta opcja nie jest używana w przypadku macierzy publikowanych zewnętrznie.
- **Dwa sterowniki fkv**: `--fkv-driver store` (domyślnie, bezpośrednio podłączone do pojedynczego `Store` w procesie oraz
  RocksDB / fjall to także wywołania bibliotek) i `--fkv-driver runtime` (tryb wdrożenia: proces
  `--fkv-shards` wątki fragmentu, żądania są kierowane według klucza, dostosowane do publicznego odczytu i zapisu C++ EloqStore
  interfejs). Każde żądanie tego ostatniego musi przejść przez kolejkę + wybudzenie międzywątkowe, natywne (macOS, plik IO przechodzi przez pulę wątków)
  Czas odczytu wynosi około 20 µs/czas, czyli o rząd wielkości wolniej niż w przypadku bezpośredniego połączenia w procesie; znacznie tańsze w systemie Linux/io_uring, ale
  Jeszcze skok. Macierz strony wykorzystuje kaliber sklepu, a kaliber środowiska wykonawczego jest używany dla „formularza wdrożenia vs C++
  Porównanie „interfejsu publicznego” (odczyt jednowątkowy w środowisku wykonawczym fkv wynosi około 50 kops/s, jednowątkowy odczyt C++ EloqStore FFI
  odłamek około 21 kops/s).
- **Odczytaj kaliber współbieżności**: numer wątku bocznego fkv = `min(readers, --read-max-concurrency)`, domyślnie
  Górny limit wynosi 4 (zgodnie z maszyną ewaluacyjną vCPU), **nie** „jeden wątek na czytnik” – przekroczenie poziomu tylko zwiększa kolejkę;
  Pamięć podręczna na wątek = 256 MiB / liczba wątków (całkowita ilość jest stała, nie niszczona przez czytniki) i używana
  `FuturesUnordered` Przechowuj `--read-window` (domyślnie 16) żądań na bieżąco. Strona RocksDB jest
  `readers` Każde `std::thread` czyta jednocześnie; jednowątkowa pętla po stronie fjall (ignorując sprzęt czytelników,
  Tylko jako kolumna referencyjna). `--no-fkv-read-threads` może powrócić do „przeplatanych N łańcuchów asynchronicznych w jednym środowisku wykonawczym”.
  Jest to głębokość potoku, a nie współbieżność i nie jest porównywalna w przypadku różnych silników.
- **len()**: Żaden z trzech silników nie ma interfejsów zliczających O(1). Ta linia oznacza współczynnik zliczania pełnej tabeli, a nie wyszukiwania O(1).