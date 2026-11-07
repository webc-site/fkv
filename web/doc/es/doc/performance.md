# Rendimiento

`fkv_bench` Pruebe cargas de escritura, lectura y escaneo en un espacio de clave fijo.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Los indicadores incluyen rendimiento, latencia en cada etapa y RSS máximo. lanzamiento de flujo de trabajo x86_64 /
`fkv_bench` binario para aarch64.