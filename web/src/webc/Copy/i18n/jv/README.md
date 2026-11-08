# Nyalin teks menyang clipboard

Klik tombol kanggo nyalin teks saka unsur sing ditemtokake.

- Ndhukung nggunakake rel kanggo nemtokake pamilih unsur kanggo disalin
- Yen ora ditemtokake, unsur sedulur sadurunge disalin kanthi gawan.
- Nested Pophover kothak pituduh
- Dhukungan internasional

## Gunakake demo

```html
<!-- Nyalin unsur tartamtu -->
<span id="target">Punika teks kanggo disalin</span>
<c-copy rel="#target"></c-copy>

<!-- Unsur sadurunge disalin kanthi gawan -->
<span>teks saka unsur sadurunge</span>
<c-copy></c-copy>
```

## paramèter

| properti | nggambaraké |
| --- | --- |
| rel | Nyalin pamilih saka unsur target. Opsional |

## gaya status

- `c-pophover.Done`: gaya kothak pituduh sawise kasil nyalin