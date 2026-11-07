# monit o pływającą bańkę

Wyświetlaj informacje o podpowiedzi po najechaniu myszką lub po aktywowaniu elementu.

- Oparty na natywnym komponencie internetowym, lekki i wydajny
- Obsługuje adaptacyjne pozycjonowanie przestrzenne w czterech kierunkach (class="top/bottom/left/right")
- Wykorzystuje tło gradientowe z czarnego matowego szkła i białe znaki, z wyjątkowymi podkreśleniami konturów fizycznych krawędzi.
- Wbudowane obliczanie adaptacyjne położenia automatycznie dostosowuje kierunek wyskakującego okienka zgodnie z pozostałą przestrzenią w rzutni
- Użyj `slot="pop"`, aby wyświetlić pływającą treść podpowiedzi

## Skorzystaj z wersji demonstracyjnej

```html
<!-- Automatycznie oblicz kierunek (zalecane, nie ma potrzeby deklarowania klasy) -->
<c-pophover>
  <button>automatyczny</button>
  <div slot="pop">To jest wskazówka dotycząca bańki</div>
</c-pophover>

<!-- stały kierunek -->
<c-pophover class="left">
  <button>Lewy</button>
  <div slot="pop">To jest wskazówka dotycząca bańki</div>
</c-pophover>

<c-pophover class="right">
  <button>Prawidłowy</button>
  <div slot="pop">To jest wskazówka dotycząca bańki</div>
</c-pophover>

<c-pophover class="bottom">
  <button>W dół</button>
  <div slot="pop">To jest wskazówka dotycząca bańki</div>
</c-pophover>

<c-pophover class="top">
  <button>znakomity</button>
  <div slot="pop">To jest wskazówka dotycząca bańki</div>
</c-pophover>
```

## Zmienne CSS

Style niestandardowe są obsługiwane za pomocą następujących zmiennych CSS:

| nazwa zmiennej | wartość domyślna | zilustrować |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Przestrzeń pomiędzy dymkiem a elementem aktywującym |
| `--pophover-margin` | `24px` | Margines bezpieczeństwa krawędzi rzutni podczas obliczania pozycjonowania adaptacyjnego |
| `--pophover-bg-top` | `#18181ce0` | Górny kolor tła gradientu bąbelkowego |
| `--pophover-bg-mid` | `#121215e5` | Kolor tła strzałek po lewej i prawej stronie bańki |
| `--pophover-bg-bottom` | `#0c0c0feb` | Kolor dolnej oraz górnej i dolnej strzałki bocznej tła gradientu bąbelka |

## Styl podpowiedzi ikony (`.Ico`)

W przypadku pływających przycisków podpowiedzi typu ikona (takich jak „Edytuj”, „Usuń” itp.) można zastosować ujednoliconą ikonę i styl najechania kursorem, dodając nazwę klasy `.Ico` do samego zewnętrznego kontenera lub komponentu.

### Przykład struktury

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">redagować</div>
  </c-pophover>
</b>
```

Podstawowy układ, rozmiar (18x18px) i zmiany koloru po najechaniu myszką zostały ujednolicone w `.Ico > c-pophover > a`. Programiści muszą jedynie osobno określić odpowiedni plik ikony, taki jak:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```