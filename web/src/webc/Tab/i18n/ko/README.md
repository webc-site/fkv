# 전환 가능한 탭

- 라벨의 선택된 상태는 아래 콘텐츠 영역과 완벽하게 통합됩니다.
- 구조적 사양: 하위 요소는 `nav > a`을 레이블 탐색으로 사용하도록 고정되었으며, 직접 하위 요소인 `b`는 콘텐츠 영역으로 사용됩니다.
- `a[value]` 태그를 `b[slot]` 콘텐츠 노드와 자동으로 연결합니다.
- `key` 속성을 ​​통해 현재 활성 페이지를 `localStorage`에 자동으로 유지하는 기능을 지원합니다.
- `value` 속성을 ​​통해 활성화된 탭 페이지 지정을 지원합니다(캐시가 없고 `value`가 있는 경우 `class="A"`이 있는 태그 또는 첫 번째 태그가 기본적으로 선택됩니다).
- 경량 Light DOM 구현, Shadow DOM 방해 없음, 스타일 사용자 정의의 높은 자유도.

## 데모 사용

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">사과</a>
    <a value="banana">바나나</a>
    <a value="orange">귤</a>
  </nav>
  <b slot="apple">사과는 영양이 풍부한 과일이다</b>
  <b slot="banana">바나나는 열대과일이에요</b>
  <b slot="orange">오렌지에는 비타민C가 풍부하다</b>
</c-tab>
```

## 재산

- `key`: localStorage 지속성에 사용되는 키 이름
- `value`: 현재 선택된 값

## 맞춤 이벤트

- `change`: 탭이 전환될 때 트리거됩니다. `e.value`는 현재 활성화된 태그의 `value` 값입니다.