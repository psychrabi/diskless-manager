import React from 'react'
import { Card } from '../ui'
import { Copy, Wrench } from 'lucide-react'
import AdminPasswordForm from './AdminPasswordForm'


const ApplicationSettings = () => {
    return (
        <Card title="Application Settings" icon={Wrench} className='bg-base-300'>
            <div className="grid grid-cols-2 gap-4">
                {/* License activation */}
                <AdminPasswordForm />
            </div>
        </Card>
    )
}
export default ApplicationSettings;