# Strona karty z rozwijanym wyborem

- Obsługuje osadzanie rozwijanego pola wyboru szerokości adaptacyjnej (`<c-select>`) w etykiecie, które będzie wyświetlane tylko wtedy, gdy aktywowana jest strona bieżącej karty.
- Specyfikacja strukturalna: element podrzędny używa elementu `nav > a` jako nawigacji po etykietach, a bezpośredni element podrzędny `b` służy jako obszar zawartości.
- Automatycznie kojarzy tag `a[value]`, `a` z `c-select` (na podstawie atrybutu wartości grupy i wartości wybranej opcji) oraz węzeł treści `b[slot]`.
- Obsługuje automatyczne utrwalanie aktualnie aktywowanego elementu w `localStorage` poprzez atrybut `key`.
- Obsługuje automatyczne zapisywanie i przywracanie historycznych podselekcji dla każdej grupy do `localStorage` przy użyciu `key + '/' + tab`.
- Obsługuje określanie aktywnej karty za pomocą atrybutu `value`.

## Skorzystaj z wersji demonstracyjnej

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>język</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>system</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript jest językiem skryptowym</b>
    <b slot="rs">Rust jest językiem programowania na poziomie systemowym</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS to graficzny system operacyjny oparty na systemie Unix</b>
    <b slot="win">Windows to system operacyjny opracowany przez firmę Microsoft</b>
  </b>
</c-tab-select>
```

## nieruchomość

- `key`: nazwa klucza używana do trwałości localStorage
- `value`: aktualnie wybrana wartość

## Niestandardowe wydarzenia

- `change`: Wywoływane podczas przełączania zakładek, `e.value` to wartość `value` aktualnie aktywowanej opcji.