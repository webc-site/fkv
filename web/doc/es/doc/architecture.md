# Arquitectura

## tiempo de ejecución

Cada fragmento tiene un subproceso de sistema operativo exclusivo y un tiempo de ejecución de compio. La solicitud ingresa a través de la cola de ingreso,
Admisión mediante máquina de estado de fragmentos, completada mediante recibo único.

## almacenamiento

El motor utiliza un árbol B+ de copia en escritura. página de datos, página de índice, cadena de desbordamiento y
El manifiesto se serializa en archivos de segmento. La sucursal se identifica con `(branch, term)` y está en manos de cada
manifiesto y instantánea de mapeo.

## IO

`fkv_io` Define el rasgo IO asincrónico. `fkv_io_compio` Implementado con compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` proporciona backends de inyección de errores y memoria, `fkv_object_s3` proporciona
Backend de objetos S3.

## persistencia

El manifiesto registra la página raíz, la raíz TTL, los metadatos de la rama y el mapeo. Cuando reabrir
Primero reproduzca el manifiesto, luego reconstruya el asignador de páginas y luego comience a atender solicitudes.