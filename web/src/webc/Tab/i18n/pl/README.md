# Przełączane zakładki

- Wybrany stan etykiety jest płynnie zintegrowany z obszarem zawartości poniżej.
- Specyfikacje strukturalne: Elementy podrzędne są przystosowane do używania `nav > a` jako nawigacji po etykietach, a bezpośredni element podrzędny `b` jest używany jako obszar zawartości.
- Automatycznie powiąż tag `a[value]` z węzłem treści `b[slot]`.
- Obsługuje automatyczne utrwalanie aktualnie aktywnej strony w `localStorage` poprzez atrybut `key`.
- Obsługuje określenie aktywowanej strony zakładki poprzez atrybut `value` (jeśli nie ma pamięci podręcznej i `value`, domyślnie wybrany zostanie tag z `class="A"` lub pierwszy tag).
- Lekka implementacja Light DOM, brak przeszkód Shadow DOM, wysoki stopień swobody w dostosowywaniu stylu.

## Skorzystaj z wersji demonstracyjnej

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">jabłko</a>
    <a value="banana">banan</a>
    <a value="orange">mandarynka</a>
  </nav>
  <b slot="apple">Jabłko to pożywny owoc</b>
  <b slot="banana">Banan to owoc tropikalny</b>
  <b slot="orange">Pomarańcze są bogate w witaminę C</b>
</c-tab>
```

## nieruchomość

- `key`: nazwa klucza używana do trwałości localStorage
- `value`: aktualnie wybrana wartość

## Niestandardowe wydarzenia

- `change`: Wywoływane po przełączeniu zakładki, `e.value` to wartość `value` aktualnie aktywowanego tagu.