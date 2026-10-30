import copy
from contextlib import chdir
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

import perf_lod as lod


def report(directory):
    directory.mkdir()
    rgb = bytes([255, 0, 0] * 4)
    rgba = bytes([255, 0, 0, 255] * 4)
    env = dict(git_commit='a'*40, git_commit_after='a'*40, git_dirty=False,
               git_dirty_after=False, build_commit='a'*40, build_dirty='false',
               build_opt_level='3', debug_assertions=False, build_rustc='rustc test',
               build_profile='bench', executable_sha256='b'*64,
               adapter={'device_type': 'IntegratedGpu', 'name': 'Hardware'})
    frame = dict(sample_id=1, completed_frame_ns=10_000_000, app_update_wall_ns=2_000_000,
                 acknowledged_sort_and_draw=True, acknowledged_projected_sort_and_draw=True,
                 render_status=[{'cloud_count':1,'sort_encoded':1,'draw_encoded':1,
                                 'projected_draw_encoded':1,'ready':True,'projected_ready':True}],
                 stream={'displayed': 10, 'selected': 12, 'cache_ready_bytes': 64,
                         'cache_reserved_bytes': 0, 'pending_downloads': 0},
                 leaf_snapshot={'backend': 'packed', 'gaussian_cameras': 1,
                                'scenes': [{'scene': 1}]}, gpu_pass_samples=[])
    cases = []
    for name in ('static', 'moving', 'near-far-return'):
        rows = [dict(copy.deepcopy(frame), sample_id=i+1) for i in range(240)]
        (directory / (name + '.ppm')).write_bytes(b'P6\n2 2\n255\n' + rgb)
        cases.append(dict(name=name, rows=rows, capture={'non_background_pixels': 4,
                          'file': name + '.ppm', 'rgba_sha256': hashlib.sha256(rgba).hexdigest()},
                          gpu_pass_reports=[{'scope': 'gaussian_projection_compute_pass'}],
                          gpu_pass_coverage={'gaussian_projection_compute_pass': {
                              'measured_frames': 240, 'frames_with_samples': 0,
                              'frames_with_valid_segment_sum': 0, 'unavailable_reason': 'unavailable'}}))
    return dict(protocol='dimensify-native-lod-offscreen-v1', schema_version=1,
                classification='hardware_performance', publication_eligible=True,
                environment=env, source={'sha256': 'c'*64, 'generated': False, 'bounds': [[0, 0, 0], [1, 1, 1]]},
                render={'backend': 'packed', 'projection_cache': True, 'preset': 'default',
                        'physical_resolution': [2, 2], 'camera_protocol': {}, 'settings': {}},
                cases=cases, resource_usage={'schema_version': 1, 'unavailable': {}, 'checkpoints': [],
                    'gpu_checkpoints': [{'phase': 'after_view:static', 'sample_id': 1, 'telemetry': {'buffers': {
                        'live_bytes': 64, 'live_buffers': 1, 'peak_live_bytes': 96,
                        'observed_bytes_total': 128, 'observed_buffers_total': 2,
                        'entries_truncated': 0, 'entries': [{'id':'one', 'bytes':64,
                            'categories':['selected', 'sorted']}],
                        'categories': {'selected': {'bytes':64,'buffers':1},
                                       'sorted': {'bytes':64,'buffers':1}},
                        'packed_source_payload_bytes': 200}}}]})


def identity(value):
    return {'protocol': value['protocol'], 'render':value['render'], 'adapter':value['environment']['adapter']}


def aggregate(values):
    keys=set(values[0])
    if any(set(value)!=keys for value in values):
        raise ValueError('Incomplete metrics across repeated runs')
    return {'metrics': values[0], 'repeat_values': {k:[v[k] for v in values] for k in keys}}


class LodResults(unittest.TestCase):
    def setUp(self):
        self.directory = Path(self.enterContext(tempfile.TemporaryDirectory()))
        self.paths = {}
        self.report_count = 0

    def report(self):
        path = self.directory / str(self.report_count)
        self.report_count += 1
        value = report(path)
        self.paths[id(value)] = path / 'raw.json'
        return value

    def results(self, reports):
        return lod.results(reports, aggregate=aggregate, identity=identity,
                           report_paths=[self.paths[id(value)] for value in reports])

    def test_three_hardware_processes_publish_scoped_units_and_partial_coverage(self):
        value=self.results([self.report() for _ in range(3)])
        metrics=value['metrics']
        self.assertEqual(metrics['static/completed-frame/median'], {'latency':{'value':10_000_000}})
        self.assertEqual(metrics['memory/tracked-gpu-buffers/after_view:static'], {'gpu-buffer-bytes':{'value':64}})
        self.assertEqual(metrics['memory/gpu-source-payload/after_view:static'], {'cache-payload-bytes':{'value':200}})
        self.assertEqual(metrics['static/gpu-pass/gaussian-projection-compute-pass/completed-frame-samples'], {'count':{'value':0}})
        self.assertFalse(any('vram' in key or 'total-gpu' in key for key in metrics))

    def test_source_requires_explicit_production_provenance(self):
        for generated in (True, None, 0):
            with self.subTest(generated=generated):
                reports = [self.report() for _ in range(3)]
                reports[0]['source']['generated'] = generated
                with self.assertRaisesRegex(ValueError, 'generated|source'):
                    self.results(reports)
        reports = [self.report() for _ in range(3)]
        del reports[0]['source']['generated']
        with self.assertRaisesRegex(ValueError, 'generated|source'):
            self.results(reports)

    def test_saved_capture_requires_real_matching_nonblank_bytes(self):
        for damage in ('missing', 'hash', 'blank', 'count', 'dimensions', 'truncated', 'unsafe'):
            with self.subTest(damage=damage):
                reports = [self.report() for _ in range(3)]
                capture = reports[0]['cases'][0]['capture']
                path = self.paths[id(reports[0])].parent / capture['file']
                if damage == 'missing': path.unlink()
                if damage == 'hash': capture['rgba_sha256'] = '0'*64
                if damage == 'blank':
                    path.write_bytes(b'P6\n2 2\n255\n' + bytes(12))
                    capture['rgba_sha256'] = hashlib.sha256(bytes([0, 0, 0, 255]*4)).hexdigest()
                if damage == 'count': capture['non_background_pixels'] = 3
                if damage == 'dimensions': path.write_bytes(b'P6\n1 4\n255\n' + bytes([255, 0, 0]*4))
                if damage == 'truncated': path.write_bytes(b'P6\n2 2\n255\n' + bytes(3))
                if damage == 'unsafe': capture['file'] = '../outside.ppm'
                with self.assertRaisesRegex(ValueError, 'capture|Capture'):
                    self.results(reports)

    def test_saved_report_resolves_capture_from_report_directory(self):
        reports = [self.report() for _ in range(3)]
        paths = [self.paths[id(value)] for value in reports]
        for value, path in zip(reports, paths):
            path.write_text(json.dumps(value))
        with chdir(self.directory):
            saved = [json.loads(path.read_text()) for path in paths]
            value = lod.results(saved, aggregate=aggregate, identity=identity, report_paths=paths)
        self.assertIn('static/completed-frame/median', value['metrics'])
        with self.assertRaisesRegex(ValueError, 'saved result path'):
            lod.results(saved, aggregate=aggregate, identity=identity)

    def test_schema_contract_rejects_missing_render_identity(self):
        for field in ('settings', 'render'):
            with self.subTest(field=field):
                reports = [self.report() for _ in range(3)]
                for value in reports:
                    if field == 'render': del value['render']
                    else: del value['render'][field]
                with self.assertRaisesRegex(ValueError, 'schema|settings'):
                    self.results(reports)

    def test_resource_schema_rejects_malformed_measurements(self):
        for damage in ('null', 'object', 'null-item', 'missing-phase', 'negative-rss',
                       'null-gpu', 'missing-telemetry', 'bad-buffer', 'bad-unavailable'):
            with self.subTest(damage=damage):
                reports = [self.report() for _ in range(3)]
                resources = reports[0]['resource_usage']
                if damage == 'null': resources['checkpoints'] = None
                if damage == 'object': resources['checkpoints'] = {}
                if damage == 'null-item': resources['checkpoints'] = [None]
                if damage == 'missing-phase': resources['checkpoints'] = [{}]
                if damage == 'negative-rss': resources['checkpoints'] = [{'phase': 'x',
                    'elapsed_ns': 1, 'rss_bytes': -1, 'virtual_memory_bytes': None, 'accumulated_cpu_ms': None}]
                if damage == 'null-gpu': resources['gpu_checkpoints'] = None
                if damage == 'missing-telemetry': resources['gpu_checkpoints'] = [{'phase': 'x'}]
                if damage == 'bad-buffer': resources['gpu_checkpoints'][0]['telemetry']['buffers']['entries'] = [None]
                if damage == 'bad-unavailable': resources['unavailable'] = None
                with self.assertRaisesRegex(ValueError, 'schema|resource_usage'):
                    self.results(reports)

    def test_rejects_different_producer_even_when_identity_omits_executable(self):
        reports=[self.report() for _ in range(3)]
        reports[1]['environment']['executable_sha256']='d'*64
        with self.assertRaisesRegex(ValueError, 'producer|Incomparable'):
            self.results(reports)

    def test_vertex_fallback_cannot_claim_actual_projected_draw(self):
        reports = [self.report() for _ in range(3)]
        reports[0]['cases'][0]['rows'][0]['acknowledged_projected_sort_and_draw'] = False
        with self.assertRaisesRegex(ValueError, 'projected'):
            self.results(reports)
        reports = [self.report() for _ in range(3)]
        reports[0]['cases'][0]['rows'][0]['render_status'][0]['projected_draw_encoded'] = 0
        with self.assertRaisesRegex(ValueError, 'projected'):
            self.results(reports)

    def test_flat_alias_or_false_projection_does_not_claim_production_lod(self):
        for mutate in (lambda r:r['render'].update(projection_cache=False),
                       lambda r:r['cases'][0]['rows'][0]['leaf_snapshot'].update(backend='snapshot'),
                       lambda r:r['cases'][0]['rows'][0]['stream'].update(displayed=0)):
            reports=[self.report() for _ in range(3)]
            mutate(reports[0])
            with self.assertRaises(ValueError): self.results(reports)

    def test_shared_buffer_categories_do_not_double_count_and_bad_sum_rejects(self):
        reports=[self.report() for _ in range(3)]
        metrics = self.results(reports)['metrics']
        self.assertEqual(metrics['memory/tracked-gpu-buffers/after_view:static'],
                         {'gpu-buffer-bytes': {'value': 64}})
        for category in ('selected', 'sorted'):
            self.assertEqual(metrics[f'memory/gpu-buffer-category/{category}/after_view:static'],
                             {'gpu-buffer-bytes': {'value': 64}})
        reports[0]['resource_usage']['gpu_checkpoints'][0]['telemetry']['buffers']['live_bytes']=128
        with self.assertRaisesRegex(ValueError, 'buffer|capacity'):
            self.results(reports)

    def test_rejects_short_blank_or_unacknowledged_replay(self):
        for mutate in (lambda r:r['cases'][0]['rows'].pop(),
                       lambda r:r['cases'][0]['capture'].update(non_background_pixels=0),
                       lambda r:r['cases'][0]['rows'][0].update(acknowledged_sort_and_draw=False),
                       lambda r:r['cases'][0]['rows'][0].update(completed_frame_ns=1)):
            reports=[self.report() for _ in range(3)]
            mutate(reports[0])
            if len(reports[0]['cases'][0]['rows'])==240 and reports[0]['cases'][0]['rows'][0]['completed_frame_ns']==1:
                for row in reports[0]['cases'][0]['rows']: row['completed_frame_ns']=1
            with self.assertRaises(ValueError): self.results(reports)

    def test_cpu_stage_wall_units_and_payload_memory_keep_distinct_scopes(self):
        reports = [self.report() for _ in range(3)]
        for value in reports:
            for case in value['cases']:
                case['rows'][-1]['cpu_timing_report'] = {'cpu_stages': {
                    'decode': {'attempts': 3, 'total_ms': 1.25, 'max_ms': .75}}}
        metrics = self.results(reports)['metrics']
        key = 'static/cpu-stage/decode/cumulative-at-case-end/'
        self.assertEqual(metrics[key+'wall-sum'], {'latency': {'value': 1_250_000}})
        self.assertEqual(metrics[key+'attempts'], {'count': {'value': 3}})

    def test_device_resource_bytes_require_exact_active_device_probe(self):
        reports = [self.report() for _ in range(3)]
        probe = {'passed':True,'requested_bytes':1048576,'observed_delta_bytes':1048576,
                 'before_bytes':4096,'after_bytes':1052672}
        for value in reports:
            value['environment']['adapter']['backend'] = 'Metal'
            measurement = {'metric':'metal_device_resource_allocated_bytes','bytes':8192,
                'reason':None,'matched_device_name':'Hardware',
                'method':'MTLCreateSystemDefaultDevice().currentAllocatedSize()',
                'allocation_probe':copy.deepcopy(probe)}
            value['device_resource_usage'] = {'allocation_probe':copy.deepcopy(measurement),
                'checkpoints':[{'phase':'after_view:static','measurement':measurement}]}
        metrics = self.results(reports)['metrics']
        self.assertEqual(metrics['memory/native-device-resource-allocated/after_view:static'],
                         {'device-resource-bytes':{'value':8192}})
        reports[0]['device_resource_usage']['checkpoints'][0]['measurement']['allocation_probe']['passed']=False
        with self.assertRaisesRegex(ValueError, 'Unverified'):
            self.results(reports)

    def test_metric_capability_change_rejects_incomplete_repeat_set(self):
        reports=[self.report() for _ in range(3)]
        reports[1]['resource_usage']['checkpoints'].append({'phase':'extra', 'elapsed_ns': 1, 'rss_bytes':42,
            'virtual_memory_bytes': None, 'accumulated_cpu_ms': None})
        with self.assertRaisesRegex(ValueError, 'Incomplete'):
            self.results(reports)


if __name__ == '__main__': unittest.main()
