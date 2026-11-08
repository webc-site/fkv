# Architektura

## czas wykonania

Każdy fragment ma ekskluzywny wątek systemu operacyjnego i środowisko wykonawcze Compio. Żądanie przechodzi przez kolejkę przychodzącą,
Wstęp przez maszynę stanu fragmentu, zakończony jednorazowym potwierdzeniem.

## składowanie

Silnik korzysta z drzewa B+ kopiowania przy zapisie. strona danych, strona indeksu, łańcuch przepełnienia i
Manifest jest serializowany w pliki segmentowe. gałąź jest identyfikowana przez `(branch, term)` i jest utrzymywana przez każdego z nich
manifestu i migawki mapowania.

## IO

`fkv_io` Definiuje cechę asynchronicznego wejścia/wyjścia. `fkv_io_compio` Zaimplementowano za pomocą compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` zapewnia backendy wstrzykiwania pamięci i błędów, `fkv_object_s3` zapewnia
Backend obiektu S3.

## trwałość

Manifest rejestruje stronę główną, katalog główny TTL, metadane gałęzi i mapowanie. Kiedy ponownie otworzyć
Najpierw odtwórz manifest, następnie odbuduj narzędzie mapujące stronę i rozpocznij obsługę żądań.