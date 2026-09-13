"""Isolated contract tests for the durable worker mailbox."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / 'scripts' / 'swarm-mail'


class SwarmMailTests(unittest.TestCase):
    def run_mail(self, mail_root, *arguments, check=True):
        return subprocess.run(
            [str(SCRIPT), *arguments], text=True, capture_output=True, check=check,
            env=dict(os.environ, WRELA_SWARM_MAIL_ROOT=str(mail_root)))

    def test_canonical_addresses_deliver_and_sender_sees_read_acknowledgement(self):
        with tempfile.TemporaryDirectory() as folder:
            mail=Path(folder)/'mail'
            identifier=self.run_mail(
                mail,'send','--from','/root/living_presentation','--to','/root/world_content',
                '--subject','Interface','--body','Ready').stdout.strip()

            pending=json.loads(self.run_mail(
                mail,'status','--agent','/root/living_presentation').stdout)
            self.assertEqual(len(pending),1)
            self.assertEqual(pending[0]['id'],identifier)
            self.assertEqual(pending[0]['recipientTask'],'/root/world_content')
            self.assertFalse(pending[0]['acknowledged'])

            delivered=json.loads(self.run_mail(
                mail,'read','--agent','/root/world_content').stdout)
            self.assertEqual(delivered,[dict(
                id=identifier,sender='living_presentation',recipient='world_content',
                subject='Interface',body='Ready',sentAt=delivered[0]['sentAt'])])
            self.assertEqual(json.loads(self.run_mail(
                mail,'read','--agent','/root/world_content').stdout),[])

            acknowledged=json.loads(self.run_mail(
                mail,'status','--agent','/root/living_presentation').stdout)
            self.assertTrue(acknowledged[0]['acknowledged'])
            self.assertIsInstance(acknowledged[0]['acknowledgedAt'],float)
            self.assertEqual(json.loads(self.run_mail(
                mail,'read','--agent','/root/world_content','--all').stdout),delivered)

    def test_roster_is_canonical_and_unknown_recipient_fails_without_writing(self):
        with tempfile.TemporaryDirectory() as folder:
            mail=Path(folder)/'mail'
            roster=json.loads(self.run_mail(mail,'roster').stdout)
            self.assertIn('/root',roster)
            self.assertIn('/root/world_content',roster)
            rejected=self.run_mail(
                mail,'send','--from','/root','--to','/root/missing_worker',
                '--subject','Bad route','--body','Must not land',check=False)
            self.assertEqual(rejected.returncode,2)
            self.assertIn("Unknown worker 'missing_worker'",rejected.stderr)
            self.assertIn('/root/world_content',rejected.stderr)
            self.assertFalse(mail.exists())


if __name__ == '__main__':
    unittest.main()
