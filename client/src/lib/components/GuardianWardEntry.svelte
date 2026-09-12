<script lang="ts">
  import SkillIcon from './SkillIcon.svelte'
  import { GUARDIAN_WARD } from '../data/abilities'
</script>

<SkillIcon {...GUARDIAN_WARD} quickslotSkill={GUARDIAN_WARD.id} />
