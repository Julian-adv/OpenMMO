# Animal and Mount Assets

## Horse

- 모델: [Horse](https://sketchfab.com/3d-models/horse-6d0f9c1ce82f41048a282b6c47a50684)
- 제작자: [MAXDESIGN-3D (@MAXDESIGN)](https://sketchfab.com/MAXDESIGN)
- 라이선스: [Creative Commons Attribution 4.0 (CC BY 4.0)](https://creativecommons.org/licenses/by/4.0/).
  Sketchfab 모델 API에서 확인. 제작자·원본 링크·라이선스·변경 내용을 함께 표시한다.
- 게시일: 2021-07-11. 사용자 제공 및 게임용 가공: 2026-09-08.
- 원본 ZIP: `assets/horse.zip` (변경 없음). 추출본: `assets/horse/source/full.fbx`,
  `assets/horse/textures/{difuse,normal,Horse_PCB}.png`.
- 게임 모델: `client/public/models/mounts/horse.glb`.
- packed Blender 작업 파일: `assets/horse/horse.blend`.
- 기승 비교 장면: `assets/horse/riding-preview.blend`, `assets/horse/riding-preview.png`.
- 게임 규칙: [MOUNTS.md](../MOUNTS.md). 고삐 아이템 출처는 [items.md](items.md).
- 기승 중 손과 입을 연결하는 고삐 줄은 자체 절차 생성(2026-09-08).
  `horseReins.ts`에서 갈색 재질·원통 구간으로 처진 곡선 두 가닥을 그린다.
  외부 모델·텍스처·AI 생성물은 없으며 저장소 라이선스를 따른다. 인벤토리 고삐 모델과는 별도다.

### 게임용 가공 (Blender 5.2.0 LTS)

기존 `knight.glb`와 함께 배치해 크기를 맞췄다. FBX 월드 좌표를 0.01배로 정규화하고,
메시·리그의 회전과 스케일을 적용하면서 본의 월드 포즈를 다시 베이크했다.
GLB 재임포트 시 idle 첫 프레임의 크기는 폭 0.636 × 길이 2.568 × 높이 2.062m,
발바닥 최저 높이는 약 0.002m다. 런타임에서 추가 확대하지 않는다.

- 메시 1개, 삼각형 14,646개. 원본은 Blender 임포트 정점 8,591개
  (Sketchfab 표기는 7,996개).
- 원본 48개 본 유지. `Spine2`에 비변형 본 `RideSeat`를 추가해 총 49개.
  기승점은 등 높이 약 1.64m에서 말의 몸통 움직임을 따라간다.
- 몸통 재질에 원본 `difuse.png` 2048²와 `normal.png` 1024²를 연결하고
  WebP q90으로 내장했다. roughness 0.85, specular 0.2, 발광 없음.
- 갈기·꼬리 `HorseHair`는 짙은 갈색 양면 재질. **[미사용]** `Horse_PCB.png`는
  별도 노멀 맵 변형으로 보존하며 게임에는 연결하지 않았다.
- 불필요한 FBX 보조 오브젝트와 빈 액션 제거. GLB에 외부 텍스처 URI나 비단위 스케일 없음.
- Blender 재임포트, 로컬 HTTPS 응답과 파일 바이트 일치, 실제 `PlayerModel`을 사용하는
  브라우저 미리보기에서 남녀 기사 탑승·달리기·하차를 확인했다.

### 애니메이션 분할

원본 `Root|Take 001|BaseLayer`는 30fps·Blender 1–938프레임에 여러 동작이 이어진
한 액션이다. 사용자 지정 구간을 기준으로 분할했다.
**Blender 프레임 = Sketchfab 뷰어 프레임 + 1**, 표의 양 끝 프레임을 포함한다.

| 클립 | Sketchfab 뷰어 | Blender | 용도 |
| --- | --- | --- | --- |
| `run` | 0–80 | 1–81 | 제자리 달리기, 약 20프레임 주기 |
| `walk` | 81–162 | 82–163 | 제자리 걷기 |
| `turn_right_180` | 163–277 | 164–278 | 180도 급회전, 게임 재생 시간 1초 |
| 중복 걷기 | 278–359 | 279–360 | 앞 걷기와 48개 본 위치가 동일해 제외 |
| `turn_right_90` | 360–435 | 361–436 | 90도 급회전, 게임 재생 시간 0.6초 |
| `idle` | 436–936 | 437–937 | 대기·고개 움직임·앞발 들기 |

GLB에는 중복 걷기를 제외한 원본 5개와 파생 좌회전 2개, 총 7개 클립을 포함한다.
원본의 추가 938프레임은 사용자 지정 끝 범위 밖이므로 제외했다.
게임에서는 속도에 따라 idle/walk/run을 전환하고 급회전 시 회전 클립을 재생한다.
2026-09-08, 자체 Blender 가공으로 `turn_left_90`·`turn_left_180`을 만들었다.
출처·라이선스는 동일한 MAXDESIGN-3D Horse, CC BY 4.0이며 새 AI 생성은 없다.
좌우 본을 교환하고 포즈를 모델의 X축으로 반전하되 각 본의 기준 축을 보정했다.
연결된 본의 위치 고정을 해제해 뒷발 끝까지 반전하며, 음수 모델 스케일은 사용하지 않는다.
네 회전 클립은 Hips의 수평 이동과 전체 yaw를 제거해 서버 방향과 중복 회전하지 않는다.
발·머리·안장 위치의 좌우 대칭과 Hips의 방향·수평 위치를 Blender에서 수치로 확인했다.

재현: `blender -b --python-exit-code 1 -P tools/blender-scripts/prepare_horse_mount.py`.
말만 재생성하려면 뒤에 `-- --horse-only`를 붙인다.
같은 스크립트가 별도의 캐릭터 기승 자세도 생성한다([animation.md](animation.md#riding)).

크레딧: “Horse” by MAXDESIGN-3D, [Sketchfab](https://sketchfab.com/3d-models/horse-6d0f9c1ce82f41048a282b6c47a50684),
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).
OnlineRPG용으로 크기·재질을 조정하고, 애니메이션을 분할하고, 기승점을 추가했다.
