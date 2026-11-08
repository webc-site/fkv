# Arquitectura

## tempo de execución

Cada fragmento ten un fío de sistema operativo exclusivo e un tempo de execución de compio. A solicitude entra pola cola de entrada,
Admisión mediante máquina de estado de fragmentos, completada a través do recibo oneshot.

## almacenamento

O motor usa a árbore B+ de copia en escritura. páxina de datos, páxina de índice, cadea de desbordamento e
O manifesto serialízase en ficheiros de segmentos. a rama é identificada por `(branch, term)` e é mantida por cada unha
manifesto e instantánea de mapeo.

## IO

`fkv_io` Define o trazo de E/S asíncrona. `fkv_io_compio` Implementado con compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` proporciona memoria e backends de inxección de fallos, `fkv_object_s3` proporciona
Backend de obxectos S3.

## persistencia

O manifesto rexistra a páxina raíz, a raíz TTL, os metadatos de rama e a asignación. Cando Reabrir
Primeiro reproduce o manifesto, despois reconstruí o mapeador de páxinas e despois comeza a atender solicitudes.