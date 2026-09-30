import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import meshy


class SubmissionTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / 'input.png').write_bytes(b'input')
        self.root_patch = patch.object(meshy, 'ROOT', self.root)
        self.root_patch.start()
        self.addCleanup(self.root_patch.stop)
        self.path = self.root / 'record.json'
        self.config = dict(parameters={'ai_model': 'meshy-7.1'}, parts=[
            dict(id='top', target_triangles=2200, views=[{'path': 'input.png'}])])
        self.record = {'tasks': []}

    def test_resume_does_not_submit_twice(self):
        with patch.object(meshy, 'request', return_value={'result': 'task-1'}) as request:
            meshy.submit(self.config, self.record, self.path)
            restored = json.loads(self.path.read_text())
            meshy.submit(self.config, restored, self.path)
            self.assertEqual(request.call_count, 1)

    def test_timeout_leaves_reconciliation_marker(self):
        with patch.object(meshy, 'request', side_effect=TimeoutError):
            with self.assertRaises(TimeoutError):
                meshy.submit(self.config, self.record, self.path)
        restored = json.loads(self.path.read_text())
        with patch.object(meshy, 'request') as request:
            with self.assertRaisesRegex(ValueError, 'Uncertain submission'):
                meshy.submit(self.config, restored, self.path)
            request.assert_not_called()

    def test_changed_input_requires_new_record(self):
        with patch.object(meshy, 'request', return_value={'result': 'task-1'}):
            meshy.submit(self.config, self.record, self.path)
        (self.root / 'input.png').write_bytes(b'changed')
        with patch.object(meshy, 'request') as request:
            with self.assertRaisesRegex(ValueError, 'Inputs/settings changed'):
                meshy.submit(self.config, self.record, self.path)
            request.assert_not_called()

    def test_missing_later_input_prevents_all_paid_requests(self):
        self.config['parts'].append(dict(id='pants', target_triangles=1300,
                                          views=[{'path': 'missing.png'}]))
        with patch.object(meshy, 'request') as request:
            with self.assertRaises(FileNotFoundError):
                meshy.submit(self.config, self.record, self.path)
            request.assert_not_called()


if __name__ == '__main__':
    unittest.main()
