import os
import tempfile
import unittest

from fileinfo import count_lines, count_words


class FileinfoTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.path = os.path.join(self.tmp.name, "sample.txt")
        with open(self.path, "w") as f:
            f.write("one two three\nfour five\n")

    def tearDown(self):
        self.tmp.cleanup()

    def test_count_words(self):
        self.assertEqual(count_words(self.path), 5)

    def test_count_lines(self):
        self.assertEqual(count_lines(self.path), 2)

    def test_missing_file_raises(self):
        missing = os.path.join(self.tmp.name, "missing.txt")
        with self.assertRaises(OSError):
            count_words(missing)
        with self.assertRaises(OSError):
            count_lines(missing)


if __name__ == "__main__":
    unittest.main()
