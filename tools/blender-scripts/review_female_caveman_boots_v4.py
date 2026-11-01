import os
import runpy
from pathlib import Path

os.environ['FEMALE_CAVEMAN_BOOT_FIT_VERSION'] = 'v4'
runpy.run_path(str(Path(__file__).with_name('review_female_caveman_boots.py')), run_name='__main__')
