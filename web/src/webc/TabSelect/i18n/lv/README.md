# Cilnes lapa ar nolaižamo izvēlni

- Atbalsta adaptīvā platuma nolaižamā atlases lodziņa (`<c-select>`) iegulšanu etiķetē, kas tiks parādīta tikai tad, kad būs aktivizēta pašreizējā cilnes lapa.
- Strukturālā specifikācija: pakārtotais elements izmanto `nav > a` kā etiķetes navigāciju, un tiešais pakārtotais elements `b` kalpo kā satura apgabals.
- Automātiski saista tagu `a[value]`, `a` ar `c-select` (pamatojoties uz grupas vērtības atribūtu un atlasītās opcijas vērtību) un `b[slot]` satura mezglu.
- Atbalsta pašlaik aktivizētā vienuma automātisku saglabāšanu līdz `localStorage`, izmantojot atribūtu `key`.
- Atbalsta katras grupas vēsturisko apakšatlases automātisku saglabāšanu un atjaunošanu uz `localStorage`, izmantojot `key + '/' + tab`.
- Atbalsta aktīvās cilnes norādīšanu, izmantojot atribūtu `value`.

## Izmantojiet demonstrāciju

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>valodu</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>sistēma</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript ir skriptu valoda</b>
    <b slot="rs">Rust ir sistēmas līmeņa programmēšanas valoda</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS ir uz Unix balstīta grafiskā operētājsistēma</b>
    <b slot="win">Windows ir Microsoft izstrādāta operētājsistēma</b>
  </b>
</c-tab-select>
```

## īpašums

- `key`: atslēgas nosaukums, kas tiek izmantots vietējās krātuves noturībai
- `value`: pašlaik atlasītā vērtība

## Pielāgoti pasākumi

- `change`: tiek aktivizēta, pārslēdzot cilnes, `e.value` ir pašlaik aktivizētās opcijas `value` vērtība.