# Tab page with drop-down selection

- Supports embedding adaptive width drop-down selection box (`<c-select>`) in the label, which will only be displayed when the current tab page is activated.
- Structural specification: The child element uses `nav > a` as the label navigation, and the direct child element `b` serves as the content area.
- Automatically associates the `a[value]` tag, `a` with `c-select` (based on the group's value attribute and the selected option value) and the `b[slot]` content node.
- Supports automatically persisting the currently activated item to `localStorage` through the `key` attribute.
- Supports automatic saving and restoring of historical sub-selections for each group to `localStorage` using `key + '/' + tab`.
- Supports specifying the active tab through the `value` attribute.

## Use the demo

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>Language</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>System</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript is a scripting language</b>
    <b slot="rs">Rust is a systems-level programming language</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS is a Unix-based graphical operating system</b>
    <b slot="win">Windows is an operating system developed by Microsoft</b>
  </b>
</c-tab-select>
```

## Property

- `key`: key name used for localStorage persistence
- `value`: currently selected value

## Custom events

- `change`: Triggered when switching tabs, `e.value` is the `value` value of the currently activated option.