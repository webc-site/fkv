# Stili tipografiċi ottimizzati għall-esperjenza tal-qari

- **Tiltir bażiku**: Adatta għal titoli, paragrafi, grassa, korsiv, barra, kodiċi inline, stampi u linji diviżorji.
- **Animazzjoni tal-link**: Uri animazzjoni tas-sottolinja meta tgħaddi.
- **Lista tal-proġetti**: Listi mhux ordnati jużaw balal, listi ordnati jużaw numri tas-serje ta' sfond fit-tond, u jappoġġjaw it-tbejjit f'diversi livelli.
- **Lista ta 'Tasks**: Tipprovdi stil ta' kaxxa ta 'kontroll, tappoġġja l-istatus ta' xogħolijiet u tlestija.
- **Suġġerimenti ta' Twissija**: Jappoġġja ħames tipi ta' kaxxi ta' twissija: Nota, Tip, Importanti, Twissija, u Attenzjoni.
- **Tabelli u Kodiċi**: Jipprovdi enfasi fuq ir-ringieli tat-tabella, u blokki tal-kodiċi b'dellijiet u sfondi gradjenti.

## Uża d-demo

```html
<div class="Md">
  <h1>titolu</h1>
  <p>Test tal-paragrafu u <a href="#">link</a></p>
  <hr>
  <h2>Titlu tat-tieni livell</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Oġġett tal-Kompitu</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Innota l-kontenut</p>
  </blockquote>
</div>
```

## klassi tal-istil

L-istili huma applikati għal elementi HTML taħt il-kontenitur `.Md`.

### Stil ta 'kaxxa ta' twissija

Żid l-isem tal-klassi li ġej fuq l-element `blockquote` bl-isem tal-klassi `.q` biex taqleb il-kaxxa tat-twissija:

- `note`: Noti
- `tip`: Tip
- `important`: Importanti
- `warning`: twissija
- `caution`: Nota