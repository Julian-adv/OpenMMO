# Asset URLs

- 애니메이션 파이프라인/매핑 규칙 문서: [ANIMATION.md](./ANIMATION.md)

## grass

- https://sketchfab.com/3d-models/realistic-grass-pack-for-games-free-9b958d613e9a44dbba580748e7a1789c
- "Grass Bursh Displacement A EO 001" (https://skfb.ly/ptwqv) by osoedwin is licensed under Creative Commons Attribution (http://creativecommons.org/licenses/by/4.0/).
- "Grass Bursh Displacement B EO 001" (https://skfb.ly/ptwQN) by osoedwin is licensed under Creative Commons Attribution (http://creativecommons.org/licenses/by/4.0/).
- "Yard Grass" (https://skfb.ly/6VxNM) by ebmclachlan is licensed under Creative Commons Attribution (http://creativecommons.org/licenses/by/4.0/).
- "Animated_grass\_-_vegetation" (https://skfb.ly/pyEME) by Savatar illusions is licensed under Creative Commons Attribution (http://creativecommons.org/licenses/by/4.0/).

## terrain

- "3D field inspection" (https://skfb.ly/oJx7N) by matousekfoto is licensed under Creative Commons Attribution (http://creativecommons.org/licenses/by/4.0/).
- https://polyhaven.com/a/gravel_floor

## human

- https://sketchfab.com/3d-models/blake-slim-walk-c4d-c076264ca7394357bf3f17837edd72c9
- https://sketchfab.com/3d-models/xbot-049e4a44ad8b449dba8a2c4824502f5c
- "Beauty Girl Exercising - Undressed Workout" (https://skfb.ly/pxpoo) by Polygonal Studios is licensed under Creative Commons Attribution (http://creativecommons.org/licenses/by/4.0/).
- "Beautiful Realistic Undressed Girls - 14 Anims" (https://skfb.ly/pxpoH) by Polygonal Studios is licensed under Creative Commons Attribution (http://creativecommons.org/licenses/by/4.0/).
- "Mutant Mixamo" (https://skfb.ly/6DvxK) by NAZTart is licensed under Creative Commons Attribution (http://creativecommons.org/licenses/by/4.0/).
- "MIXAMO" (https://skfb.ly/ottKO) by sdhkim is licensed under Creative Commons Attribution (http://creativecommons.org/licenses/by/4.0/).
- "Bandit Armor and Clothes - Game Model" (https://skfb.ly/6UVot) by wolkoed is licensed under Creative Commons Attribution (http://creativecommons.org/licenses/by/4.0/).
- Maria https://sketchfab.com/3d-models/maria-a04cac95ab8046e4bbdc9dec30c7d92d
- dying https://sketchfab.com/3d-models/dying-98a1d5b2288d49d993039cb161913cd3
- medieval_knight https://sketchfab.com/3d-models/medieval-knight-sculpture-game-ready-6cdd055b4afa41eb9360dbbfe75c7f10

### animations

- mixamo.com에서 받은 fbx를 blender에서 scale 10으로 임포트한다

- Medea By M. Arrebola https://www.mixamo.com/#/?page=1&query=&type=Character
- Walking https://www.mixamo.com/#/?page=1&query=walk&type=Motion%2CMotionPack
- Catwalk Walk Forward https://www.mixamo.com/#/?page=2&query=walk&type=Motion%2CMotionPack
- Standing Torch Walk Forward
- Catwalk Walking

- Run (허리구부리고) https://www.mixamo.com/#/?page=1&query=run&type=Motion%2CMotionPack
- Slow Run https://www.mixamo.com/#/?page=1&query=run&type=Motion%2CMotionPack
- Jogging https://www.mixamo.com/#/?page=1&query=jog&type=Motion%2CMotionPack

- Standing Idle https://www.mixamo.com/#/?page=1&query=idle&type=Motion%2CMotionPack
- Happy Idle
- Dwarf Idle
- Offensive Idle https://www.mixamo.com/#/?page=2&query=idle&type=Motion%2CMotionPack
- Sword And Shield Idle

- Sword and Shield Slash https://www.mixamo.com/#/?page=1&query=slash&type=Motion%2CMotionPack

## Monster

- SCP939 https://sketchfab.com/3d-models/scp939-79a749a5073b453d9d85875797bf45d7

## Item

- sword.glb https://www.fab.com/listings/5fe82d66-eaac-48e0-899d-1fedacdf409a

# Blender

- Use version 4.2.12 LTS

## Import tips

- .glb를 import 할 때 bone shape scale을 0.01로 하면 거대한 구체가 나타나는 것을 방지할 수 있다.

## Export tips

- Backface Culling: Material Properties → Settings → Backface Culling 켜기(뒷면 제거).
- 노드 에디터(Shader/Geometry Nodes) 활성화
  - Alpha가 의도치 않게 들어가 있는지: Base Color 텍스처에 알파가 섞여 Alpha에 연결돼 있지 않은지 확인.
- .glb 내보내기 시 권장 옵션(Blender glTF 2.0 Exporter)
  - Apply Modifiers: 켜기
  - (노멀맵 쓴다면) Tangents: 켜기
