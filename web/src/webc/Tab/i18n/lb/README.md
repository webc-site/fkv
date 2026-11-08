# Schaltbar Tabs

- De gewielten Zoustand vum Label ass nahtlos integréiert mam Inhaltsberäich hei drënner.
- Strukturell Spezifikatioune: D'Kandelementer si fixéiert fir `nav > a` als Labelnavigatioun ze benotzen, an den direkten Kannerelement `b` gëtt als Inhaltsberäich benotzt.
- Ass den `a[value]` Tag automatesch mam `b[slot]` Inhaltsknuet.
- Ënnerstëtzt automatesch déi aktuell aktiv Säit op `localStorage` duerch den `key` Attribut.
- Ënnerstëtzt d'Spezifikatioun vun der aktivéierter Tab-Säit duerch den `value` Attribut (wann et kee Cache gëtt an `value`, gëtt den Tag mat `class="A"` oder den éischte Tag als Standard gewielt).
- Liichtgewiicht Liicht DOM Implementatioun, keng Shadow DOM Behënnerung, héije Grad vu Fräiheet an der Stil Personnalisatioun.

## Benotzt d'Demo

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">apel</a>
    <a value="banana">banana</a>
    <a value="orange">tangerine</a>
  </nav>
  <b slot="apple">Apple ass eng nährstoffaarme Fruucht</b>
  <b slot="banana">Bananen ass eng tropesch Uebst</b>
  <b slot="orange">Orangen si reich an Vitamin C</b>
</c-tab>
```

## Propriétéit

- `key`: Schlësselnumm benotzt fir d'LokalStorage Persistenz
- `value`: aktuell gewielte Wäert

## Benotzerdefinéiert Evenementer

- `change`: Ausgeléist wann d'Tab gewiesselt gëtt, ass `e.value` de `value` Wäert vum aktuell aktivéierten Tag.