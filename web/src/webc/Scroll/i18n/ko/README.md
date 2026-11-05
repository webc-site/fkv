# 가상 스크롤바

- **양방향 스크롤**: 수직(`c-vs`) 및 수평(`c-hs`) 스크롤을 제공합니다.
- **크기 응답**: 콘텐츠 및 영역 크기 변경을 모니터링하고 스크롤 막대 크기 및 위치를 자동으로 업데이트하며 오버플로가 없으면 자동으로 숨겨집니다.
- **대화형 지원**: 휠 스크롤, 슬라이더 드래그 및 트랙 클릭 위치 지정을 지원합니다.
- **상태 숨기기**: 마우스를 올리거나 스크롤할 때 슬라이더를 표시하고 넓히고, 움직이지 않을 때 자동으로 숨깁니다.

## 수직 스크롤

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

## 가로 스크롤

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

## 스타일 부분

`::part()`을 통해 Shadow DOM 내부 요소의 스타일을 맞춤설정하세요.

- `::part(scroll)`: 콘텐츠 영역을 스크롤합니다.
- `::part(bar)`: 스크롤 막대 트랙.
- `::part(si)`: 스크롤 막대 슬라이더.
- `::part(i)`: 내부 래핑 레이어.
- `::part(drag)`: 드래그 상태의 스크롤 막대 트랙.

## 스타일 수업

- `.drag`: `body` 요소에 적용되어 드래그 스크롤 상태임을 나타냅니다.

## 사용자 정의 속성

- `--si-bg`: 슬라이더 배경색.
- `--si-anim`: 슬라이더 애니메이션.
- `--cursorScrollhSvg`: 가로 스크롤 막대를 가리키면 마우스 커서입니다.
- `--cursorScrollvSvg`: 세로 스크롤 막대를 가리키면 마우스 커서입니다.