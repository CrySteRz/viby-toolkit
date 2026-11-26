import os
import tempfile
import unittest

from docshare import open_attachment, read_download
from docshare.attachments import OutsideRoot
from docshare.downloads import list_downloads


class DocshareTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = self.tmp.name
        with open(os.path.join(self.root, "guide.txt"), "wb") as f:
            f.write(b"hello")

    def tearDown(self):
        self.tmp.cleanup()

    def test_download_reads_file(self):
        self.assertEqual(read_download(self.root, "guide.txt"), b"hello")

    def test_download_missing_file(self):
        with self.assertRaises(FileNotFoundError):
            read_download(self.root, "nope.txt")

    def test_list_downloads(self):
        self.assertEqual(list_downloads(self.root), ["guide.txt"])

    def test_attachment_opens(self):
        with open_attachment(self.root, "guide.txt") as f:
            self.assertEqual(f.read(), b"hello")

    def test_attachment_refuses_parent_folder(self):
        with self.assertRaises(OutsideRoot):
            open_attachment(self.root, "../guide.txt")


if __name__ == "__main__":
    unittest.main()
