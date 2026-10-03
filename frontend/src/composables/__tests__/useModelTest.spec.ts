import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent } from 'vue'
import { useModelTest, type StartTestParams } from '../useModelTest'

const mocks = vi.hoisted(() => ({
  testModel: vi.fn(),
  testModelFailover: vi.fn(),
  getRequestTrace: vi.fn(),
  showSuccess: vi.fn(),
  showError: vi.fn(),
}))

vi.mock('@/api/endpoints/providers', () => ({
  testModel: mocks.testModel,
  testModelFailover: mocks.testModelFailover,
}))
vi.mock('@/api/requestTrace', () => ({
  requestTraceApi: { getRequestTrace: mocks.getRequestTrace },
}))
vi.mock('../useToast', () => ({
  useToast: () => ({ success: mocks.showSuccess, error: mocks.showError }),
}))

const params: StartTestParams = {
  mode: 'global',
  modelName: 'test-model',
  displayLabel: 'Responses',
  apiFormat: 'openai:responses',
  endpointId: 'responses-endpoint',
  requestBody: { model: 'test-model', input: 'Hello', stream: false },
}

const cleanups: Array<() => void> = []

function mountModelTest() {
  let modelTest!: ReturnType<typeof useModelTest>
  const app = createApp(defineComponent({
    setup() {
      modelTest = useModelTest({ providerId: () => 'provider-1' })
      return () => null
    },
  }))
  app.mount(document.createElement('div'))
  cleanups.push(() => app.unmount())
  return modelTest
}

beforeEach(() => {
  vi.clearAllMocks()
  mocks.getRequestTrace.mockResolvedValue({ candidates: [] })
})

afterEach(() => {
  cleanups.splice(0).forEach(cleanup => cleanup())
})

describe('model test failure details', () => {
  it.each(['direct', 'global', 'pool'] as const)('preserves a manually edited request model in %s mode', async (mode) => {
    mocks.testModel.mockResolvedValue({ success: true, model: 'custom-model' })
    mocks.testModelFailover.mockResolvedValue({ success: true, model: 'custom-model', attempts: [] })
    await mountModelTest().startTest({ ...params, mode, mappedModelName: 'mapped-model',
      requestBody: { model: 'custom-model', input: 'Hello' } })
    const api = mode === 'direct' ? mocks.testModel : mocks.testModelFailover
    expect(api).toHaveBeenCalledWith(expect.objectContaining({
      request_body: { model: 'custom-model', input: 'Hello' } }), expect.any(Object))
  })

  it('applies mapping when the request model has not been edited', async () => {
    mocks.testModelFailover.mockResolvedValue({ success: true, attempts: [] })
    await mountModelTest().startTest({ ...params, applyModelMapping: true })
    const request = mocks.testModelFailover.mock.calls[0]?.[0]
    expect(request.apply_model_mapping).toBe(true)
  })
  it.each(['direct', 'global', 'pool'] as const)(
    'forwards the client protocol independently of the upstream endpoint in %s mode',
    async (mode) => {
      const requestBody = {
        model: 'test-model',
        messages: [{ role: 'user', content: 'Hello' }],
        max_tokens: 16,
        stream: false,
      }
      mocks.testModel.mockResolvedValue({ success: true, model: 'test-model' })
      mocks.testModelFailover.mockResolvedValue({ success: true, model: 'test-model', attempts: [] })
      const modelTest = mountModelTest()
      await modelTest.startTest({
        ...params,
        mode,
        apiFormat: 'openai:chat',
        clientApiFormat: 'claude:messages',
        endpointId: 'chat-endpoint',
        requestBody,
      })

      const testApi = mode === 'direct' ? mocks.testModel : mocks.testModelFailover
      expect(testApi).toHaveBeenCalledWith(expect.objectContaining({
        api_format: 'openai:chat',
        client_api_format: 'claude:messages',
        endpoint_id: 'chat-endpoint',
        request_body: requestBody,
      }), expect.objectContaining({ signal: expect.any(AbortSignal) }))
      expect(modelTest.testResult.value?.success).toBe(true)
    },
  )

  it('keeps an HTTP error in the dialog and retains its request trace', async () => {
    mocks.testModelFailover.mockRejectedValue({
      response: { status: 503, data: { error: { message: 'Model test execution failed' } } },
    })
    const trace = { candidates: [{ status: 'failed', error_message: 'Connection failed' }] }
    mocks.getRequestTrace.mockResolvedValue(trace)
    const modelTest = mountModelTest()
    await modelTest.startTest(params)

    expect(modelTest.dialogOpen.value).toBe(true)
    expect(modelTest.testing.value).toBe(false)
    expect(modelTest.testResult.value).toMatchObject({
      success: false,
      error: 'Model test execution failed',
    })
    expect(modelTest.requestId.value).toMatch(/^provider-test-/)
    expect(modelTest.testTrace.value).toEqual(trace)
  })

  it('retains the request and upstream response when an attempt fails', async () => {
    const result = {
      success: false,
      model: params.modelName,
      attempts: [{
        status: 'failed',
        request_headers: { authorization: '[REDACTED]' },
        request_body: params.requestBody,
        response_body: { error: { message: 'Provider rejected request' } },
      }],
      total_candidates: 1,
      total_attempts: 1,
      error: 'Provider rejected request',
    }
    mocks.testModelFailover.mockResolvedValue(result)
    const modelTest = mountModelTest()
    await modelTest.startTest(params)

    expect(modelTest.dialogOpen.value).toBe(true)
    expect(modelTest.testResult.value).toEqual(result)
    expect(modelTest.requestId.value).toMatch(/^provider-test-/)
  })

  it('does not restore a cancelled test when its trace refresh finishes', async () => {
    mocks.testModelFailover.mockRejectedValue(new Error('Connection failed'))
    let resolveTrace!: (trace: unknown) => void
    mocks.getRequestTrace.mockReturnValue(new Promise(resolve => { resolveTrace = resolve }))
    const modelTest = mountModelTest()
    const pendingTest = modelTest.startTest(params)
    await vi.waitFor(() => expect(mocks.getRequestTrace).toHaveBeenCalled())

    modelTest.resetState()
    resolveTrace({ candidates: [] })
    await pendingTest

    expect(modelTest.dialogOpen.value).toBe(false)
    expect(modelTest.testResult.value).toBeNull()
    expect(modelTest.requestId.value).toBeNull()
    expect(mocks.showError).not.toHaveBeenCalled()
  })

  it('does not overwrite a newer test when an earlier trace refresh finishes', async () => {
    mocks.testModelFailover.mockRejectedValueOnce(new Error('Connection failed'))
    let resolveOldTrace!: (trace: unknown) => void
    mocks.getRequestTrace.mockReturnValueOnce(new Promise(resolve => { resolveOldTrace = resolve }))
    const modelTest = mountModelTest()
    const oldTest = modelTest.startTest(params)
    await vi.waitFor(() => expect(mocks.getRequestTrace).toHaveBeenCalled())

    let resolveNewTest!: (result: unknown) => void
    mocks.testModelFailover.mockReturnValueOnce(new Promise(resolve => { resolveNewTest = resolve }))
    const newTest = modelTest.startTest({ ...params, modelName: 'new-model' })
    const newRequestId = modelTest.requestId.value
    resolveOldTrace({ candidates: [] })
    await oldTest

    expect(modelTest.testing.value).toBe(true)
    expect(modelTest.testResult.value).toBeNull()
    expect(modelTest.requestId.value).toBe(newRequestId)
    expect(mocks.showError).not.toHaveBeenCalled()

    resolveNewTest({ success: true, model: 'new-model', attempts: [] })
    await newTest
    expect(modelTest.testResult.value?.model).toBe('new-model')
    expect(modelTest.testing.value).toBe(false)
  })
})
