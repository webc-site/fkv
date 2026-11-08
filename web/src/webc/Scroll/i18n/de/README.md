# virtuelle Bildlaufleiste

- **Zwei-Wege-Scrollen**: Bietet vertikales (`c-vs`) und horizontales (`c-hs`) Scrollen.
- **Größenreaktion**: Überwachen Sie Inhalts- und Bereichsgrößenänderungen, aktualisieren Sie automatisch die Größe und Position der Bildlaufleiste und blenden Sie sie automatisch aus, wenn kein Überlauf vorliegt.
- **Interaktive Unterstützung**: Unterstützt das Scrollen mit dem Rad, das Ziehen des Schiebereglers und die Positionierung per Mausklick.
- **Status ausblenden**: Blenden Sie den Schieberegler ein, wenn Sie mit der Maus darüber fahren oder scrollen, und blenden Sie ihn automatisch aus, wenn er sich nicht bewegt.

## vertikales Scrollen

```html
<div style="height:300px;border:1px solid #000">
  <c-vs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-vs>
</div>
<style>b { display: flex; border: 2px solid #090; padding: 30px; margin: 30px; }</style>
```

## horizontaler Bildlauf

```html
<div style="width:300px;border:1px solid #000">
  <c-hs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-hs>
</div>
<style>b { display: flex; border: 2px solid #090; margin: 30px; padding: 30px; }</style>
```

## Stilteile

Passen Sie den Stil der internen Shadow DOM-Elemente über `::part()` an:

- `::part(scroll)`: Scrollen Sie durch den Inhaltsbereich.
- `::part(bar)`: Bildlaufleistenspur.
- `::part(si)`: Schieberegler für die Bildlaufleiste.
- `::part(i)`: Interne Verpackungsschicht.
- `::part(drag)`: Die Bildlaufleistenspur im Ziehzustand.

## Stilklasse

- `.drag`: Wird auf das Element `body` angewendet und zeigt an, dass es sich in einem Drag-Scroll-Zustand befindet.

## Benutzerdefinierte Eigenschaften

- `--si-bg`: Hintergrundfarbe des Schiebereglers.
- `--si-anim`: Slider-Animation.
- `--cursorScrollhSvg`: Der Mauszeiger, wenn der Mauszeiger über die horizontale Bildlaufleiste bewegt wird.
- `--cursorScrollvSvg`: Der Mauszeiger, wenn der Mauszeiger über die vertikale Bildlaufleiste bewegt wird.