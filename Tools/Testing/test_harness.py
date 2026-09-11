"""Fast tests of report, baseline and exclusivity policies. No native app required."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from report import report
from runner import compare_perf, difference
from runtime import performance_lease, write_json


class HarnessPolicyTests(unittest.TestCase):
    def test_difference_decodes_nested_saved_state(self):
        import base64
        encoded=lambda n:base64.b64encode(json.dumps({'trust':n}).encode()).decode()
        self.assertIn('state.expedition.trust',difference({'expedition':encoded(1)},{'expedition':encoded(2)}))

    def test_failure_report_is_readable_and_escapes_authored_text(self):
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)
            report(path,dict(kind='run',passed=False,error='Expected creature',runs=[dict(name='<script>bad</script>',passed=False,failure='fear: expected <1')]))
            text=(path/'index.html').read_text()
            self.assertIn('Expected creature',text)
            self.assertIn('&lt;script&gt;',text)
            self.assertNotIn('<script>bad',text)

    def test_baselines_reject_regression_and_incompatible_workload(self):
        baseline=dict(machine={'chip':'test'},owner='cave',performanceKind='cpu',workload='one',measurements=[dict(name='one',definitionDigest='a',p95=1)])
        with tempfile.TemporaryDirectory() as folder:
            file=Path(folder)/'result.json';write_json(file,baseline)
            compare_perf(copy.deepcopy(baseline),file)
            slow=copy.deepcopy(baseline);slow['measurements'][0]['p95']=2
            with self.assertRaisesRegex(RuntimeError,'regression'):
                compare_perf(slow,file)
            changed=copy.deepcopy(baseline);changed['measurements'][0]['definitionDigest']='b'
            with self.assertRaisesRegex(RuntimeError,'definition changed'):
                compare_perf(changed,file)

    def test_performance_runs_cannot_overlap(self):
        with performance_lease():
            with self.assertRaisesRegex(RuntimeError,'Another performance'):
                with performance_lease():
                    self.fail('overlapping measurements allowed')


if __name__=='__main__':
    unittest.main()
