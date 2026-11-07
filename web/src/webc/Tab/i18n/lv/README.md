# Pārslēdzamas cilnes

- Atlasītais etiķetes stāvoklis ir nemanāmi integrēts tālāk esošajā satura apgabalā.
- Strukturālās specifikācijas: pakārtotie elementi ir fiksēti, lai izmantotu `nav > a` kā etiķetes navigāciju, un tiešais pakārtotais elements `b` tiek izmantots kā satura apgabals.
- Automātiski saistiet tagu `a[value]` ar `b[slot]` satura mezglu.
- Atbalsta automātisku pašreizējās aktīvās lapas saglabāšanu līdz `localStorage`, izmantojot atribūtu `key`.
- Atbalsta aktivizētās cilnes lapas norādīšanu, izmantojot atribūtu `value` (ja nav kešatmiņas un `value`, pēc noklusējuma tiks atlasīts tags ar `class="A"` vai pirmais tags).
- Viegls Light DOM ieviešana, bez Shadow DOM šķēršļiem, augsta stila pielāgošanas brīvība.

## Izmantojiet demonstrāciju

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">ābolu</a>
    <a value="banana">banāns</a>
    <a value="orange">mandarīns</a>
  </nav>
  <b slot="apple">Ābols ir barojošs auglis</b>
  <b slot="banana">Banāns ir tropu auglis</b>
  <b slot="orange">Apelsīni ir bagāti ar C vitamīnu</b>
</c-tab>
```

## īpašums

- `key`: atslēgas nosaukums, kas tiek izmantots vietējās krātuves noturībai
- `value`: pašlaik atlasītā vērtība

## Pielāgoti pasākumi

- `change`: tiek aktivizēts, kad tiek pārslēgta cilne, `e.value` ir pašlaik aktivizētā taga `value` vērtība.