#!/usr/bin/env ruby
# Independent stdlib validation of the compact K64/F64 WP4-M JSONL.

require 'digest'
require 'json'

SIZES = [1_048_576, 10_485_760, 104_857_600].freeze
OPS = %w[write edit-same edit-plus1-early edit-plus1-middle].freeze
ARMS = (SIZES.map { |size| [size, 'write'] } + OPS.drop(1).map { |operation| [SIZES.last, operation] }).freeze
ENGINE_OPS = OPS.zip(%w[full same-middle plus1-early plus1-middle]).to_h.freeze
PROFILE = 'b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1'
HEX64 = /\A[0-9a-f]{64}\z/
APPROVED_BUDGET = {
  'approved' => true,
  'old_reference_count' => 5_410_816,
  'insertion_ordinal' => 2_705_408,
  'rebuilt_reference_occurrences' => 2_705_409,
  'changed_leaves' => 42_273,
  'changed_branches' => 673,
  'mapping_objects' => 42_947,
  'canonical_mapping_bytes' => 186_891_342,
  'latency_projection' => false,
}.freeze

def stats(values)
  sorted = values.sort
  { 'count' => sorted.length, 'median_ns' => sorted[sorted.length / 2], 'min_ns' => sorted.first,
    'max_ns' => sorted.last, 'spread_ns' => sorted.last - sorted.first }
end

def slope(numerator, denominator)
  { 'numerator' => numerator, 'denominator' => denominator,
    'decimal' => denominator.zero? ? 'Unavailable' : format('%.6f', numerator.fdiv(denominator)) }
end

def changed_counts(old_references, insertion_ordinal)
  total = (old_references + 64) / 64
  first = insertion_ordinal / 64
  leaves = total - first
  branches = 0
  while total > 64
    total = (total + 63) / 64
    first /= 64
    branches += total - first
  end
  [leaves, branches]
end

def analyze(rows, raw_sha256)
  errors = []
  campaign = rows.select { |r| %w[warmup measured].include?(r['row_kind']) }
  roundtrips = rows.select { |r| r['row_kind'] == 'roundtrip-check' }
  unknown = rows.reject { |r| %w[warmup measured roundtrip-check].include?(r['row_kind']) }
  errors << "campaign row count #{campaign.length} != 24" unless campaign.length == 24
  errors << 'warmup row count != 6' unless campaign.count { |r| r['row_kind'] == 'warmup' } == 6
  errors << 'measured row count != 18' unless campaign.count { |r| r['row_kind'] == 'measured' } == 18
  errors << "roundtrip row count #{roundtrips.length} != 3" unless roundtrips.length == 3
  errors << "unknown row_kind count #{unknown.length}" unless unknown.empty?

  schedule = Hash.new(0)
  campaign.each { |r| schedule[[r['size_bytes'], r['operation'], r['row_kind'], r['sample_index']]] += 1 }
  expected = {}
  ARMS.each do |size, operation|
    expected[[size, operation, 'warmup', 0]] = 1
    (1..3).each { |sample| expected[[size, operation, 'measured', sample]] = 1 }
  end
  errors << 'campaign schedule is not exactly 6 arms x (1+3)' unless schedule == expected
  rt_schedule = roundtrips.map { |r| [r['size_bytes'], r['operation'], r['row_kind'], r['sample_index']] }.sort_by(&:first)
  errors << 'roundtrip schedule is not one write per size' unless rt_schedule == SIZES.map { |size| [size, 'write', 'roundtrip-check', nil] }

  rows.each_with_index do |row, index|
    label = "row #{index}"
    errors << "#{label}: schema" unless row['schema'] == 'wp4m-fixed-radix-acceptance-row-v1'
    errors << "#{label}: purpose" unless row['purpose'] == 'fixed_radix_acceptance'
    errors << "#{label}: milestone" unless row['milestone'] == 'WP4-M-FIXED-RADIX'
    errors << "#{label}: status" unless row['status'] == 'PASS'
    errors << "#{label}: profile" unless row['candidate'] == 'K64-F64' && row['profile_id'] == PROFILE
    errors << "#{label}: qualification/promotion" unless row['qualification'] == false && row['promotion'] == false
    errors << "#{label}: operation mapping" unless row['engine_operation'] == ENGINE_OPS[row['operation']]
    scope = row['row_kind'] == 'roundtrip-check' ? 'complete-roundtrip' : 'capture-only'
    errors << "#{label}: validation scope" unless row['validation_scope'] == scope
    %w[fixture_sha256 source_fingerprint executable_sha256 runner_sha256].each do |field|
      errors << "#{label}: custody hash" unless HEX64.match?(row[field].to_s)
    end
    %w[root_id transition_id ordered_closure_digest].each do |field|
      errors << "#{label}: result identity" unless HEX64.match?(row[field].to_s)
    end
    errors << "#{label}: CDC count" unless row['actual_cdc_references'] == row['expected_cdc_references']
    errors << "#{label}: runner ceiling" unless row['runner_wall_ceiling_seconds'] == 120 && row['runner_command_ceiling_seconds'] == 60
    exact = { 'transactions' => 1, 'commits' => 1, 'commit_dispatches' => 1, 'commit_returns' => 1,
              'commit_return_successes' => 1, 'commit_return_errors' => 0, 'q_current' => 0 }
    errors << "#{label}: transaction/COMMIT/Q" unless exact.all? { |field, value| row[field] == value }
    errors << "#{label}: COMMIT timer equation" unless row['commit_timer_equation_matches'] == true
    errors << "#{label}: durable timer equation" unless row['durable_phase_sum_matches'] == true
    %w[capture_publish_wall_ns complete_lifecycle_total_wall_ns].each do |field|
      errors << "#{label}: #{field}" unless row[field].is_a?(Integer) && row[field].positive?
    end
    next if row['operation'] == 'write'
    %w[pre_edit_database_sha256 pre_edit_authority_sha256 pre_edit_expectations_sha256].each do |field|
      errors << "#{label}: pre-edit custody" unless HEX64.match?(row[field].to_s)
    end
  end

  errors << 'multiple executable identities' unless rows.map { |r| r['executable_sha256'] }.uniq.length == 1
  errors << 'multiple runner identities' unless rows.map { |r| r['runner_sha256'] }.uniq.length == 1
  SIZES.each do |size|
    sized = rows.select { |r| r['size_bytes'] == size }
    errors << "#{size}: multiple source identities" unless sized.map { |r| r['source_fingerprint'] }.uniq.length == 1
    errors << "#{size}: multiple fixture identities" unless sized.map { |r| [r['fixture'], r['fixture_sha256']] }.uniq.length == 1
    ARMS.select { |arm_size, _| arm_size == size }.each do |_, operation|
      arm = campaign.select { |r| r['size_bytes'] == size && r['operation'] == operation }
      result_ids = arm.map { |r| [r['root_id'], r['transition_id'], r['ordered_closure_digest']] }.uniq
      errors << "#{size}/#{operation}: unstable result identities" unless result_ids.length == 1
      next if operation == 'write'
      base_ids = arm.map { |r| [r['pre_edit_database_sha256'], r['pre_edit_authority_sha256'], r['pre_edit_expectations_sha256']] }.uniq
      errors << "#{size}/#{operation}: unstable pre-edit custody" unless base_ids.length == 1
    end
    write_ids = campaign.select { |r| r['size_bytes'] == size && r['operation'] == 'write' }
                        .map { |r| [r['root_id'], r['transition_id'], r['ordered_closure_digest']] }.uniq
    roundtrip_ids = roundtrips.select { |r| r['size_bytes'] == size }
                             .map { |r| [r['root_id'], r['transition_id'], r['ordered_closure_digest']] }.uniq
    errors << "#{size}: roundtrip identity differs from write" unless write_ids == roundtrip_ids
  end

  suffix_summary = []
  [SIZES.last].each do |size|
    OPS.last(2).each do |operation|
      arm = campaign.select { |r| r['size_bytes'] == size && r['operation'] == operation }
      counter_fields = %w[suffix_references suffix_bytes suffix_objects pages branches mapping_bytes_rewritten]
      errors << "#{size}/#{operation}: unstable suffix counters" unless arm.map { |r| counter_fields.map { |f| r[f] } }.uniq.length == 1
      next if arm.empty?
      row = arm.first
      model = row['suffix_model'] || {}
      old = model['old_references']
      position = operation.end_with?('early') ? 0 : (old.is_a?(Integer) ? old / 2 : -1)
      leaves, branches = old.is_a?(Integer) && old >= 0 ? changed_counts(old, position) : [-1, -1]
      expected_model = {
        'kind' => 'ordinal-fixed-radix-suffix-linear-v1', 'old_references' => old,
        'insertion_ordinal' => position, 'rewritten_references' => old.is_a?(Integer) ? old - position : -1,
        'rewritten_raw_bytes' => row['suffix_bytes'], 'authenticated_objects' => row['suffix_objects'],
        'rewritten_pages' => row['pages'], 'rewritten_branches' => row['branches'],
        'rewritten_mapping_bytes' => row['mapping_bytes_rewritten'],
      }
      errors << "#{size}/#{operation}: suffix model fields" unless model == expected_model
      errors << "#{size}/#{operation}: suffix reference equation" unless row['suffix_references'] == expected_model['rewritten_references']
      errors << "#{size}/#{operation}: fixed-radix topology equation" unless row['pages'] == leaves && row['branches'] == branches
      errors << "#{size}/#{operation}: unstable suffix model" unless arm.all? { |r| r['suffix_model'] == model }
      suffix_summary << {
        'size_bytes' => size, 'operation' => operation, 'source_suffix_references' => row['suffix_references'],
        'rebuilt_reference_occurrences' => row['suffix_references'].to_i + 1,
        'rewritten_raw_bytes' => row['suffix_bytes'], 'authenticated_objects' => row['suffix_objects'],
        'changed_leaves' => row['pages'], 'changed_branches' => row['branches'],
        'mapping_objects' => row['pages'].to_i + row['branches'].to_i + 1,
        'canonical_mapping_bytes' => row['mapping_bytes_rewritten'],
      }
    end
  end

  arms = []
  index = {}
  ARMS.each do |size, operation|
    measured = campaign.select { |r| r['size_bytes'] == size && r['operation'] == operation && r['row_kind'] == 'measured' }
    next unless measured.length == 3
    item = {
      'size_bytes' => size, 'operation' => operation,
      'publish_wall' => stats(measured.map { |r| r['capture_publish_wall_ns'] }),
      'complete_wall' => stats(measured.map { |r| r['complete_lifecycle_total_wall_ns'] }),
      'mapping_bytes_rewritten' => measured.first['mapping_bytes_rewritten'],
    }
    arms << item
    index[[size, operation]] = item
  end
  slopes = ['write'].each_with_object([]) do |operation, output|
    [[SIZES[0], SIZES[1]], [SIZES[1], SIZES[2]], [SIZES[0], SIZES[2]]].each do |small_size, large_size|
      small, large = index.values_at([small_size, operation], [large_size, operation])
      next unless small && large
      output << {
        'operation' => operation, 'from_size_bytes' => small_size, 'to_size_bytes' => large_size,
        'publish_wall' => slope(large['publish_wall']['median_ns'], small['publish_wall']['median_ns']),
        'complete_wall' => slope(large['complete_wall']['median_ns'], small['complete_wall']['median_ns']),
        'mapping_bytes' => slope(large['mapping_bytes_rewritten'], small['mapping_bytes_rewritten']),
      }
    end
  end
  alarms = [SIZES.last].each_with_object([]) do |size, output|
    full = index[[size, 'write']]
    OPS.last(2).each do |operation|
      edit = index[[size, operation]]
      next unless full && edit
      output << { 'size_bytes' => size, 'operation' => operation,
                  'publish_to_write_percent' => slope(edit['publish_wall']['median_ns'] * 100, full['publish_wall']['median_ns'])['decimal'],
                  'binding' => false }
    end
  end

  {
    'schema' => 'wp4m-fixed-radix-analysis-v1', 'status' => errors.empty? ? 'PASS' : 'FAIL',
    'reasons' => errors.uniq.sort,
    'row_counts' => { 'campaign' => campaign.length, 'warmup' => campaign.count { |r| r['row_kind'] == 'warmup' },
                      'measured' => campaign.count { |r| r['row_kind'] == 'measured' }, 'roundtrip' => roundtrips.length },
    'custody' => { 'raw_jsonl_sha256' => raw_sha256, 'executable_sha256' => rows.first&.dig('executable_sha256'),
                   'runner_sha256' => rows.first&.dig('runner_sha256') },
    'arms' => arms, 'slopes' => slopes, 'suffix_models' => suffix_summary,
    'local_five_percent_alarm' => alarms, 'approved_100_gib_middle_budget' => APPROVED_BUDGET,
    'routine_contract' => { 'sizes_bytes' => SIZES, 'capture_rows' => 24, 'roundtrip_rows' => 3,
                            'runner_ceiling_seconds' => 120, 'command_ceiling_seconds' => 60,
                            'size_512_mib_closes_wp4m' => false },
    'disposition' => { 'qualification' => false, 'promotion' => false, 'directory_default' => 'DIR256K-unmeasured-fallback' },
  }
end

def self_test_rows
  h = ->(value) { Digest::SHA256.hexdigest(value) }
  references = { SIZES[0] => 53, SIZES[1] => 531, SIZES[2] => 5_284 }
  rows = []
  SIZES.each do |size|
    ids = {}
    ARMS.select { |arm_size, _| arm_size == size }.each do |_, operation|
      ids[operation] = %w[root transition closure].map { |kind| h.call("#{kind}-#{size}-#{operation}") }
      [['warmup', [0]], ['measured', (1..3).to_a]].each do |kind, samples|
        samples.each do |sample|
          old = references[size]
          position = operation.end_with?('early') ? 0 : old / 2
          suffix = operation.start_with?('edit-plus1') ? old - position : 0
          leaves, branches = suffix.positive? ? changed_counts(old, position) : [1, 1]
          row = {
            'schema' => 'wp4m-fixed-radix-acceptance-row-v1', 'purpose' => 'fixed_radix_acceptance', 'milestone' => 'WP4-M-FIXED-RADIX',
            'status' => 'PASS', 'candidate' => 'K64-F64', 'profile_id' => PROFILE, 'qualification' => false, 'promotion' => false,
            'size_bytes' => size, 'operation' => operation, 'engine_operation' => ENGINE_OPS[operation], 'row_kind' => kind,
            'sample_index' => sample, 'validation_scope' => 'capture-only', 'fixture' => "S-#{size}",
            'fixture_sha256' => h.call("fixture-#{size}"), 'source_fingerprint' => h.call("source-#{size}"),
            'executable_sha256' => h.call('exe'), 'runner_sha256' => h.call('runner'),
            'pre_edit_database_sha256' => h.call("db-#{size}-#{operation}"), 'pre_edit_authority_sha256' => h.call("authority-#{size}-#{operation}"),
            'pre_edit_expectations_sha256' => h.call("expectations-#{size}-#{operation}"), 'root_id' => ids[operation][0],
            'transition_id' => ids[operation][1], 'ordered_closure_digest' => ids[operation][2],
            'actual_cdc_references' => old + (suffix.positive? ? 1 : 0), 'expected_cdc_references' => old + (suffix.positive? ? 1 : 0),
            'runner_wall_ceiling_seconds' => 120, 'runner_command_ceiling_seconds' => 60,
            'transactions' => 1, 'commits' => 1, 'commit_dispatches' => 1, 'commit_returns' => 1,
            'commit_return_successes' => 1, 'commit_return_errors' => 0, 'q_current' => 0,
            'commit_timer_equation_matches' => true, 'durable_phase_sum_matches' => true,
            'capture_publish_wall_ns' => size + sample * 1_000 + OPS.index(operation) * 100,
            'complete_lifecycle_total_wall_ns' => size * 2 + sample * 1_000 + OPS.index(operation) * 100,
            'suffix_references' => suffix, 'suffix_bytes' => suffix * 20_000, 'suffix_objects' => leaves * 2 + branches,
            'pages' => leaves, 'branches' => branches,
            'mapping_bytes_rewritten' => (suffix + 1) * 68 + leaves * 28 + branches * 69 + 49,
          }
          if suffix.positive?
            row['suffix_model'] = {
              'kind' => 'ordinal-fixed-radix-suffix-linear-v1', 'old_references' => old, 'insertion_ordinal' => position,
              'rewritten_references' => suffix, 'rewritten_raw_bytes' => row['suffix_bytes'],
              'authenticated_objects' => row['suffix_objects'], 'rewritten_pages' => leaves, 'rewritten_branches' => branches,
              'rewritten_mapping_bytes' => row['mapping_bytes_rewritten'],
            }
          end
          rows << row
        end
      end
    end
    base = rows.find { |r| r['size_bytes'] == size && r['operation'] == 'write' }
    rows << base.merge('row_kind' => 'roundtrip-check', 'sample_index' => nil, 'validation_scope' => 'complete-roundtrip',
                       'capture_publish_wall_ns' => size, 'complete_lifecycle_total_wall_ns' => size * 3)
  end
  rows
end

if ARGV == ['--self-test']
  rows = self_test_rows
  hash = Digest::SHA256.hexdigest('self-test')
  raise 'valid fixture rejected' unless analyze(rows, hash)['status'] == 'PASS'
  broken = rows.map(&:dup)
  broken.first['commits'] = 2
  raise 'invalid fixture accepted' unless analyze(broken, hash)['status'] == 'FAIL'
  puts 'PASS'
  exit
end

abort("usage: #{$PROGRAM_NAME} RAW.jsonl | --self-test") unless ARGV.length == 1
begin
  raw = File.binread(ARGV.first)
  rows = raw.lines.reject { |line| line.strip.empty? }.map { |line| JSON.parse(line) }
  result = analyze(rows, Digest::SHA256.hexdigest(raw))
rescue StandardError => e
  result = { 'schema' => 'wp4m-fixed-radix-analysis-v1', 'status' => 'FAIL', 'reasons' => ["input: #{e}"] }
end
puts JSON.generate(result)
exit(result['status'] == 'PASS' ? 0 : 1)
