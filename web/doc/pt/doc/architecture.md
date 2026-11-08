# Arquitetura

## tempo de execução

Cada fragmento possui um thread de sistema operacional exclusivo e um tempo de execução compio. A solicitação entra pela fila de entrada,
Admissão por máquina de estado de fragmento, concluída por meio de recibo único.

## armazenar

O mecanismo usa árvore B + copy-on-write. página de dados, página de índice, cadeia de overflow e
O manifesto é serializado em arquivos de segmento. filial é identificada por `(branch, term)` e é mantida por cada
manifesto e instantâneo de mapeamento.

## IO

`fkv_io` Define a característica de E/S assíncrona. `fkv_io_compio` Implementado com compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` fornece back-ends de memória e injeção de falhas, `fkv_object_s3` fornece
Back-end do objeto S3.

## persistência

O manifesto registra página raiz, raiz TTL, metadados de ramificação e mapeamento. Quando reabrir
Primeiro reproduza o manifesto, depois reconstrua o mapeador de páginas e comece a atender solicitações.