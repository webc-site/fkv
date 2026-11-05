# Switchable tabs

- The selected state of the label is seamlessly integrated with the content area below.
- Structural specifications: The child elements are fixed to use `nav > a` as the label navigation, and the direct child element `b` is used as the content area.
- Automatically associate the `a[value]` tag with the `b[slot]` content node.
- Supports automatically persisting the current active page to `localStorage` through the `key` attribute.
- Supports specifying the activated tab page through the `value` attribute (if there is no cache and `value`, the tag with `class="A"` or the first tag will be selected by default).
- Lightweight Light DOM implementation, no Shadow DOM obstruction, high degree of freedom in style customization.

## Use the demo

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">Apple</a>
    <a value="banana">Banana</a>
    <a value="orange">Tangerine</a>
  </nav>
  <b slot="apple">Apple is a nutritious fruit</b>
  <b slot="banana">Banana is a tropical fruit</b>
  <b slot="orange">Oranges are rich in vitamin C</b>
</c-tab>
```

## Property

- `key`: key name used for localStorage persistence
- `value`: currently selected value

## Custom events

- `change`: Triggered when the tab is switched, `e.value` is the `value` value of the currently activated tag.