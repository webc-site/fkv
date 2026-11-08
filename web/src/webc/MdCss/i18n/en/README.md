# Typography styles optimized for reading experience

- **Basic typesetting**: Adapt to titles, paragraphs, bold, italics, strikethrough, inline code, pictures and dividing lines.
- **Link animation**: Display underline animation when hovering.
- **Project list**: Unordered lists use bullets, ordered lists use rounded background serial numbers, and support multi-level nesting.
- **Task List**: Provides check box style, supports to-do and completed status.
- **Warning Tips**: Supports five types of warning boxes: Note, Tip, Important, Warning, and Attention.
- **Tables and Code**: Provides table row hover highlighting, and code blocks with shadows and gradient backgrounds.

## Use the demo

```html
<div class="Md">
  <h1>Title</h1>
  <p>Paragraph text and <a href="#">link</a></p>
  <hr>
  <h2>Second level title</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Task Item</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Note content</p>
  </blockquote>
</div>
```

## Style Class

Styles are applied to HTML elements under the `.Md` container.

### Alert box style

Add the following class name on the `blockquote` element with the `.q` class name to toggle the alert box:

- `note`: Notes
- `tip`: Tip
- `important`: Important
- `warning`: warning
- `caution`: Note