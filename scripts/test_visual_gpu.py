"""A visual CI lane must prove that its selected GPU cases actually ran."""

import importlib.util
from pathlib import Path
import unittest


SCRIPT = Path(__file__).with_name("visual-gpu.py")


class GpuInventoryTests(unittest.TestCase):
    def module(self):
        self.assertTrue(SCRIPT.is_file(), "GPU inventory/count guard is missing")
        spec = importlib.util.spec_from_file_location("visual_gpu", SCRIPT)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def test_missing_or_extra_gpu_tests_fail_inventory(self):
        module = self.module()
        expected = {"case_a", "case_b"}
        module.validate_inventory("case_a: test\ncase_b: test\n", expected)
        for actual in [
            "",
            "case_a: test\n",
            "case_a: test\ncase_b: test\nfixture: test\n",
        ]:
            with self.assertRaises(ValueError):
                module.validate_inventory(actual, expected)

    def test_duplicate_inventory_is_not_counted_as_coverage(self):
        with self.assertRaises(ValueError):
            self.module().validate_inventory("case_a: test\ncase_a: test\n", {"case_a"})

    def test_zero_or_ignored_execution_is_not_success(self):
        module = self.module()
        module.validate_summary(
            "test result: ok. 21 passed; 0 failed; 0 ignored; 5 filtered out; finished in 1s\n",
            21,
        )
        for output in [
            "",
            "test result: ok. 0 passed; 0 failed; 0 ignored; 21 filtered out; finished in 0s\n",
            "test result: ok. 0 passed; 0 failed; 21 ignored; 0 filtered out; finished in 0s\n",
            "test result: FAILED. 20 passed; 1 failed; 0 ignored; 0 filtered out; finished in 1s\n",
        ]:
            with self.assertRaises(ValueError):
                module.validate_summary(output, 21)

    def test_portable_suite_excludes_external_fixtures(self):
        module = self.module()
        self.assertEqual(len(module.RASTER_CASES), 24)
        self.assertEqual(len(module.BUFFER_CASES), 1)
        self.assertTrue(
            all("producer_bounds" not in name for name in module.RASTER_CASES)
        )
        self.assertIn("--ignored", module.test_args("raster"))
        self.assertIn("--skip", module.test_args("raster"))
        self.assertIn("producer_bounds::", module.test_args("raster"))
        self.assertIn("--test-threads=1", module.test_args("raster"))


if __name__ == "__main__":
    unittest.main()
