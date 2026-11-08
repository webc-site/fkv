# Copïo testun i'r clipfwrdd

Cliciwch y botwm i gopïo testun yr elfen benodol.

- Yn cefnogi defnyddio rel i nodi'r dewisydd elfen i'w gopïo
- Pan na chaiff ei nodi, mae'r elfen brawd neu chwaer flaenorol yn cael ei chopïo yn ddiofyn.
- Blwch annog Pophover yn nythu
- Cefnogaeth ryngwladol

## Defnyddiwch y demo

```html
<!-- Copïo elfennau penodol -->
<span id="target">Dyma'r testun i'w gopïo</span>
<c-copy rel="#target"></c-copy>

<!-- Mae elfennau blaenorol yn cael eu copïo yn ddiofyn -->
<span>testun yr elfen flaenorol</span>
<c-copy></c-copy>
```

## paramedr

| eiddo | darlunio |
| --- | --- |
| rel | Copïwch ddewiswr yr elfen darged. Dewisol |

## arddull statws

- `c-pophover.Done`: arddull blwch prydlon ar ôl copïo llwyddiannus