# Drop-down selection, width adapts to current item

The width of native `select` is determined by the longest option text by default. When switching short options, there is too much white space, which affects the appearance.

This component transparently covers the native `select` above the `b` tag, allowing the width to accurately adapt to the length of the currently selected text.

- Maintain native `select` dropdown menu styles and accessibility support on all terminals
- Supports native interactive operations such as complete keyboard Tab navigation and direction key switching.
- Adapt the width to the current option to eliminate excess white space

## Use the demo

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```